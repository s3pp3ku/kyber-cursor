#pragma once
#include <cstddef>
#include <cstdint>

// Layout mirrors the Rust repr(C) types. No C++ containers cross this ABI.
struct KyberTrail;
struct KyberTrailBounds { double x, y, width, height; };
struct KyberTrailRaster {
    const uint8_t* data;
    uint32_t width, height;
    KyberTrailBounds bounds;
};
extern "C" {
KyberTrail* kyber_trail_create();
void kyber_trail_destroy(KyberTrail*);
void kyber_trail_clear(KyberTrail*);
bool kyber_trail_color(KyberTrail*, const char*);
void kyber_trail_sample(KyberTrail*, double now, double x, double y);
KyberTrailBounds kyber_trail_bounds(KyberTrail*, double now);
size_t kyber_trail_points(const KyberTrail*);
KyberTrailRaster kyber_trail_render(KyberTrail*, double now, KyberTrailBounds view, bool light);
}
