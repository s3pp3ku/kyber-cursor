// Experimental Hyprland adapter. Rust prepares the cursor frames and trails.
// The compositor moves one cursor buffer; local motion events feed Rust trails.
#include <hyprland/src/plugins/PluginAPI.hpp>
#include <hyprland/src/pointer/PointerManager.hpp>
#include <hyprland/src/pointer/cursor/CursorManager.hpp>
#include <hyprland/src/managers/eventLoop/EventLoopManager.hpp>
#include <hyprland/src/managers/EventManager.hpp>
#include <hyprland/src/managers/SessionLockManager.hpp>
#include <hyprland/src/render/Renderer.hpp>
#include <hyprland/src/render/gl/GLTexture.hpp>
#include <hyprland/src/render/pass/TexPassElement.hpp>
#include <hyprland/src/output/Monitor.hpp>
#include <drm_fourcc.h>
#include "trail.h"
#include <array>
#include <chrono>
#include <cmath>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <set>
#include <stdexcept>

namespace {
using Clock = std::chrono::steady_clock;
using Buffer = Pointer::Cursor::CCursorBuffer;
std::array<std::vector<SP<Buffer>>, 2> frames;
std::array<std::vector<SP<Buffer>>, 2> holdFrames;
uint32_t holdFrameMs = 80;
int currentHoldPhase = -1;
Vector2D hotspot;
float scale = 2.F;
int bank = 0;
bool active = false, replacing = false;
double progress = 0;
Clock::time_point lastTick;
Clock::time_point startedAt;
std::set<uint32_t> held;
SP<Aquamarine::IBuffer> savedBuffer;
WP<Desktop::View::CWLSurface> savedSurface;
Vector2D savedHotspot;
float savedScale = 1.F;
SP<CEventLoopTimer> timer;
SP<CEventLoopTimer> recolorTimer;
std::filesystem::path loadedGeneration;
CHyprSignalListener buttonListener, cursorListener, lockListener;
CHyprSignalListener moveListener, renderListener;
KyberTrail* trail = nullptr;
SP<CEventLoopTimer> trailTimer;
KyberTrailBounds previousTrailBounds = {};
bool trailEnabled = false;
uint64_t trailRenders = 0;
bool enabled = true;
uint64_t pendingCursorChange = 0;
Vector2D lastAudioPosition;
Vector2D swipeStartPosition;
Clock::time_point lastAudioMove{};
Clock::time_point swipeWindowStart{};
Clock::time_point lastSwipeSound{};
double swipeDistance = 0;
bool haveAudioPosition = false;
SP<SHyprCtlCommand> controlCommand;

void audioEvent(const std::string& data) {
    g_pEventManager->postEvent({"kyberbutton", data});
}

void detectSwipe(Vector2D position) {
    const auto now = Clock::now();
    if (!haveAudioPosition) {
        haveAudioPosition = true;
        lastAudioPosition = position;
        lastAudioMove = now;
        return;
    }

    const auto previous = lastAudioMove;
    const double elapsed = std::chrono::duration<double>(now - previous).count();
    const auto previousPosition = lastAudioPosition;
    const double distance = std::hypot(position.x - lastAudioPosition.x, position.y - lastAudioPosition.y);
    lastAudioPosition = position;
    lastAudioMove = now;
    if (elapsed <= 0 || elapsed > 0.08 || distance > 600) {
        swipeWindowStart = {};
        swipeDistance = 0;
        return;
    }

    if (swipeWindowStart == Clock::time_point{}) {
        swipeWindowStart = previous;
        swipeStartPosition = previousPosition;
    }
    const double window = std::chrono::duration<double>(now - swipeWindowStart).count();
    if (window > 0.08) {
        swipeWindowStart = now;
        swipeStartPosition = position;
        swipeDistance = 0;
        return;
    }
    swipeDistance = std::hypot(position.x - swipeStartPosition.x, position.y - swipeStartPosition.y);
    if (window < 0.035 || swipeDistance < 96) return;

    const double speed = swipeDistance / window;
    const bool cooledDown = lastSwipeSound == Clock::time_point{} ||
        now - lastSwipeSound >= std::chrono::milliseconds(450);
    if (speed >= 1700 && cooledDown) {
        audioEvent("swipe," + std::to_string(std::clamp(speed, 650.0, 10000.0)));
        lastSwipeSound = now;
    }
    swipeWindowStart = {};
    swipeDistance = 0;
}

bool configuredActive() {
    const auto* config = std::getenv("XDG_CONFIG_HOME");
    const auto* home = std::getenv("HOME");
    if (!home && !config) return false;
    const auto root = config ? std::filesystem::path(config) : std::filesystem::path(home) / ".config";
    return std::filesystem::exists(root / "kyber-cursor/active");
}

bool configuredTrail() {
    const auto* config = std::getenv("XDG_CONFIG_HOME");
    const auto* home = std::getenv("HOME");
    if (!home && !config) return false;
    const auto root = config ? std::filesystem::path(config) : std::filesystem::path(home) / ".config";
    std::ifstream file(root / "kyber-cursor/trail-mode");
    std::string mode;
    file >> mode;
    return mode != "off";
}

double trailNow() {
    return std::chrono::duration<double>(Clock::now().time_since_epoch()).count();
}

void damageTrail() {
    if (!trail) return;
    // Damage the preceding bounds as well, so disappearing pixels get erased.
    if (previousTrailBounds.width > 0 && previousTrailBounds.height > 0)
        g_pHyprRenderer->damageBox(CBox(previousTrailBounds.x, previousTrailBounds.y, previousTrailBounds.width, previousTrailBounds.height));
    previousTrailBounds = kyber_trail_bounds(trail, trailNow());
    if (previousTrailBounds.width > 0 && previousTrailBounds.height > 0) {
        g_pHyprRenderer->damageBox(CBox(previousTrailBounds.x, previousTrailBounds.y, previousTrailBounds.width, previousTrailBounds.height));
        if (trailTimer) trailTimer->updateTimeout(std::chrono::milliseconds(16));
    } else if (trailTimer) trailTimer->updateTimeout(std::nullopt);
}

void clearTrail() {
    if (trail) kyber_trail_clear(trail);
    damageTrail();
}

void trailColor() {
    clearTrail();
    std::ifstream file(loadedGeneration / "color.txt");
    std::string color;
    file >> color;
    if (trail) kyber_trail_color(trail, color.c_str());
}

void drawTrail() {
    if (!enabled || !trailEnabled || !trail || g_pSessionLockManager->isSessionLocked()) return;
    const auto monitor = g_pHyprRenderer->m_renderData.pMonitor.lock();
    if (!monitor) return;
    const auto image = kyber_trail_render(trail, trailNow(),
        {monitor->m_position.x, monitor->m_position.y, monitor->m_size.x, monitor->m_size.y}, bank == 1);
    if (!image.data || !image.width || !image.height) return;
    // Upload immediately: the pixels are borrowed from Rust until its next render.
    const auto texture = makeShared<Render::GL::CGLTexture>(DRM_FORMAT_ABGR8888,
        const_cast<uint8_t*>(image.data), image.width * 4, Vector2D(static_cast<double>(image.width), static_cast<double>(image.height)), false, false);
    const auto box = CBox(image.bounds.x - monitor->m_position.x, image.bounds.y - monitor->m_position.y,
        image.bounds.width, image.bounds.height).scale(monitor->m_scale);
    CTexPassElement::SRenderData data{};
    data.tex = texture;
    data.box = box;
    g_pHyprRenderer->addPassElement(makeUnique<CTexPassElement>(std::move(data)));
    ++trailRenders;
}

void rememberCursor() {
    const auto& image = Pointer::mgr()->currentCursorImage();
    savedBuffer = image.pBuffer;
    savedSurface = image.surface;
    savedHotspot = image.hotspot;
    savedScale = image.scale;
}

void present() {
    if (!active || replacing) return;
    const auto eased = 1.0 - std::pow(1.0 - progress, 3.0);
    const auto index = static_cast<size_t>(std::lround(eased * (frames[bank].size() - 1)));
    auto buffer = frames[bank].at(index);
    currentHoldPhase = -1;
    if (progress >= 1 && !held.empty() && !holdFrames[bank].empty()) {
        const auto elapsed = std::max<int64_t>(0, std::chrono::duration_cast<std::chrono::milliseconds>(Clock::now() - startedAt).count() - 150);
        currentHoldPhase = static_cast<int>(static_cast<size_t>(elapsed / holdFrameMs) % holdFrames[bank].size());
        buffer = holdFrames[bank].at(static_cast<size_t>(currentHoldPhase));
    }
    replacing = true;
    Pointer::mgr()->setCursorBuffer(buffer, hotspot, scale);
    replacing = false;
}

void restore() {
    if (!active) return;
    if (pendingCursorChange) {
        g_pEventLoopManager->removeDoLater(pendingCursorChange);
        pendingCursorChange = 0;
    }
    active = false;
    progress = 0;
    currentHoldPhase = -1;
    if (timer) timer->updateTimeout(std::nullopt);
    replacing = true;
    if (auto surface = savedSurface.lock())
        Pointer::mgr()->setCursorSurface(surface, savedHotspot);
    else if (savedBuffer)
        Pointer::mgr()->setCursorBuffer(savedBuffer, savedHotspot, savedScale);
    else
        Pointer::Cursor::mgr()->setCursorFromName("left_ptr");
    replacing = false;
    savedBuffer.reset();
    savedSurface.reset();
}

void tick() {
    if (!active) return;
    const auto now = Clock::now();
    const double dt = std::clamp(std::chrono::duration<double>(now - lastTick).count(), 0.0, 0.05);
    lastTick = now;
    progress = std::clamp(progress + (held.empty() ? -dt / 0.11 : dt / 0.15), 0.0, 1.0);
    if (held.empty() && progress <= 0) { restore(); return; }
    present();
    // Cursor movement is still entirely compositor-native. While held, swap
    // pre-rendered Rust halo phases at a low rate; never poll pointer position.
    if (!held.empty() && progress >= 1) {
        if (!holdFrames[bank].empty()) timer->updateTimeout(std::chrono::milliseconds(holdFrameMs));
        return;
    }
    timer->updateTimeout(std::chrono::milliseconds(16));
}

void loadFrames() {
    const auto root = std::filesystem::path(KYBER_NATIVE_DIR);
    const auto dir = std::filesystem::canonical(std::filesystem::exists(root / "current") ? root / "current" : root);
    std::ifstream manifest(dir / "frames.txt");
    std::array<std::vector<SP<Buffer>>, 2> nextFrames;
    std::array<std::vector<SP<Buffer>>, 2> nextHoldFrames;
    float nextScale = 0;
    std::string magic;
    uint32_t width = 0, height = 0, hx = 0, hy = 0, count = 0;
    uint32_t nextHoldCount = 0, nextHoldMs = 80;
    manifest >> magic >> width >> height >> hx >> hy >> nextScale >> count;
    if (!manifest || magic != "KYBER_NATIVE_V1" || width == 0 || height == 0 || width > 256 || height > 256 ||
        hx >= width || hy >= height || !std::isfinite(nextScale) || nextScale < 1 || nextScale > 4 || count < 2 || count > 64)
        throw std::runtime_error("Invalid Kyber native cursor manifest");
    std::string extension;
    if (manifest >> extension) {
        manifest >> nextHoldCount >> nextHoldMs;
        if (!manifest || extension != "KYBER_HALO_LOOP_V1" || nextHoldCount < 2 || nextHoldCount > 64 || nextHoldMs < 40 || nextHoldMs > 250)
            throw std::runtime_error("Invalid Kyber halo loop manifest");
    }
    const auto nextHotspot = Vector2D(static_cast<double>(hx), static_cast<double>(hy)) / nextScale;
    const auto load = [&](auto& destination, bool loop, uint32_t frameCount) {
        for (int b = 0; b < 2; ++b) {
            for (uint32_t i = 0; i < frameCount; ++i) {
                const auto name = std::format("{}{}{:02}.png", b == 0 ? "dark" : "light", loop ? "-hold-" : "-", i);
                auto* surface = cairo_image_surface_create_from_png((dir / name).c_str());
                if (cairo_surface_status(surface) != CAIRO_STATUS_SUCCESS ||
                    cairo_image_surface_get_width(surface) != static_cast<int>(width) ||
                    cairo_image_surface_get_height(surface) != static_cast<int>(height)) {
                    cairo_surface_destroy(surface);
                    throw std::runtime_error("Invalid Kyber native cursor frame: " + name);
                }
                destination[b].push_back(makeShared<Buffer>(surface, Vector2D(static_cast<double>(width), static_cast<double>(height)), nextHotspot));
                cairo_surface_destroy(surface);
            }
        }
    };
    load(nextFrames, false, count);
    load(nextHoldFrames, true, nextHoldCount);
    frames = std::move(nextFrames);
    holdFrames = std::move(nextHoldFrames);
    holdFrameMs = nextHoldMs;
    scale = nextScale;
    hotspot = nextHotspot;
    loadedGeneration = dir;
}
}

APICALL EXPORT std::string PLUGIN_API_VERSION() { return HYPRLAND_API_VERSION; }

APICALL EXPORT PLUGIN_DESCRIPTION_INFO PLUGIN_INIT(HANDLE handle) {
    if (std::string(__hyprland_api_get_hash()) != __hyprland_api_get_client_hash())
        throw std::runtime_error("Kyber native trial: compositor/header ABI mismatch");
    loadFrames();
    enabled = configuredActive();
    trailEnabled = configuredTrail();
    trail = kyber_trail_create();
    trailColor();
    trailTimer = makeShared<CEventLoopTimer>(std::nullopt, [](SP<CEventLoopTimer>, void*) { damageTrail(); }, nullptr);
    g_pEventLoopManager->addTimer(trailTimer);
    timer = makeShared<CEventLoopTimer>(std::nullopt, [](SP<CEventLoopTimer>, void*) { tick(); }, nullptr);
    g_pEventLoopManager->addTimer(timer);
    recolorTimer = makeShared<CEventLoopTimer>(std::chrono::milliseconds(250), [](SP<CEventLoopTimer> self, void*) {
        try {
            const auto nextEnabled = configuredActive();
            if (!nextEnabled && enabled) { held.clear(); restore(); clearTrail(); audioEvent("cancel"); }
            enabled = nextEnabled;
            const auto nextTrail = configuredTrail();
            if (!nextTrail && trailEnabled) clearTrail();
            trailEnabled = nextTrail;
            const auto current = std::filesystem::path(KYBER_NATIVE_DIR) / "current";
            if (std::filesystem::exists(current) && std::filesystem::canonical(current) != loadedGeneration) {
                loadFrames();
                trailColor();
                present();
            }
        } catch (const std::exception& error) {
            // Keep the last complete set if a recolor fails. Do not throw into Hyprland.
            std::cerr << "kyber-native: " << error.what() << '\n';
        }
        self->updateTimeout(std::chrono::milliseconds(250));
    }, nullptr);
    g_pEventLoopManager->addTimer(recolorTimer);
    lockListener = g_pSessionLockManager->m_events.lock.listen([]() {
        held.clear(); restore(); clearTrail(); audioEvent("cancel");
    });
    moveListener = Event::bus()->m_events.input.mouse.move.listen([](Vector2D position, Event::SCallbackInfo&) {
        if (!enabled || held.empty() || g_pSessionLockManager->isSessionLocked()) {
            haveAudioPosition = false;
            swipeWindowStart = {};
            swipeDistance = 0;
            return;
        }
        detectSwipe(position);
        if (!trailEnabled || !active || held.empty()) return;
        kyber_trail_sample(trail, trailNow(), position.x, position.y);
        damageTrail();
    });
    renderListener = Event::bus()->m_events.render.stage.listen([](eRenderStage stage) {
        // Keep the real cursor (including software-cursor fallbacks) above trails.
        if (stage == RENDER_POST_WINDOWS) drawTrail();
    });
    buttonListener = Event::bus()->m_events.input.mouse.button.listen([](IPointer::SButtonEvent event, Event::SCallbackInfo&) {
        if (event.button < 0x110 || event.button > 0x112) return;
        if (!enabled || g_pSessionLockManager->isSessionLocked()) return;
        if (event.state == WL_POINTER_BUTTON_STATE_PRESSED) {
            if (!held.insert(event.button).second) return;
            if (!active) {
                rememberCursor(); active = true; progress = 0; startedAt = Clock::now();
                clearTrail();
                if (trailEnabled) {
                    const auto position = Pointer::mgr()->position();
                    kyber_trail_sample(trail, trailNow(), position.x, position.y);
                }
            }
            haveAudioPosition = false;
            swipeWindowStart = {};
            swipeDistance = 0;
            audioEvent("down," + std::to_string(event.button));
        } else {
            if (!held.erase(event.button)) return;
            if (held.empty()) {
                haveAudioPosition = false;
                swipeWindowStart = {};
                swipeDistance = 0;
            }
            audioEvent("up," + std::to_string(event.button));
        }
        lastTick = Clock::now();
        present();
        timer->updateTimeout(std::chrono::milliseconds(16));
    });
    cursorListener = Pointer::mgr()->m_events.cursorChanged.listen([]() {
        if (!active || replacing || pendingCursorChange) return;
        // setCursorBuffer emits while clearing the previous cursor as well
        // as after setting its replacement. Never re-enter that mutation.
        pendingCursorChange = g_pEventLoopManager->doLater([]() {
            pendingCursorChange = 0;
            if (!active || replacing) return;
            rememberCursor();
            present();
        });
    });
    HyprlandAPI::addDispatcherV2(handle, "kyber-native", [](std::string command) -> SDispatchResult {
        if (command == "dark") bank = 0;
        else if (command == "light") bank = 1;
        else if (command == "stop") { held.clear(); restore(); clearTrail(); audioEvent("cancel"); return {}; }
        else return {.success = false, .error = "Use kyber-native dark|light|stop"};
        present();
        return {};
    });
    controlCommand = HyprlandAPI::registerHyprCtlCommand(handle, {
        .name = "kybernative",
        .exact = false,
        .fn = [](eHyprCtlOutputFormat, std::string request) {
            const auto space = request.find(' ');
            const auto command = space == std::string::npos ? "status" : request.substr(space + 1);
            if (command == "dark") bank = 0;
            else if (command == "light") bank = 1;
            else if (command == "stop") { held.clear(); restore(); clearTrail(); audioEvent("cancel"); }
            else if (command != "status") return std::string("error: use kybernative dark|light|stop|status");
            present();
            return std::string("mode=") + (bank == 0 ? "dark" : "light") + " generation=" + loadedGeneration.string()
                + " shimmer_frames=" + std::to_string(holdFrames[bank].size()) + " shimmer_phase=" + std::to_string(currentHoldPhase)
                + " active=" + (active ? "yes" : "no") + " enabled=" + (enabled ? "yes" : "no") + " protocol=2 trails=1"
                + " trail=" + (trailEnabled ? "on" : "off") + " trail_points=" + std::to_string(kyber_trail_points(trail))
                + " trail_renders=" + std::to_string(trailRenders);
        },
    });
    return {"kyber-native-trial", "Rust native cursor, trails and audio event bridge", "Kyber Cursor", "0.5"};
}

APICALL EXPORT void PLUGIN_EXIT() {
    buttonListener.reset();
    cursorListener.reset();
    lockListener.reset();
    moveListener.reset();
    renderListener.reset();
    clearTrail();
    audioEvent("cancel");
    if (timer) { timer->cancel(); g_pEventLoopManager->removeTimer(timer); timer.reset(); }
    if (recolorTimer) { recolorTimer->cancel(); g_pEventLoopManager->removeTimer(recolorTimer); recolorTimer.reset(); }
    if (trailTimer) { trailTimer->cancel(); g_pEventLoopManager->removeTimer(trailTimer); trailTimer.reset(); }
    if (trail) { kyber_trail_destroy(trail); trail = nullptr; }
    held.clear();
    restore();
    frames = {};
    holdFrames = {};
    controlCommand.reset();
}
