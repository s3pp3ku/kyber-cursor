#include "../trail.h"
#include <cassert>
#include <cmath>
#include <iostream>
#include <type_traits>

int main() {
    static_assert(std::is_standard_layout_v<KyberTrailBounds>);
    static_assert(sizeof(KyberTrailBounds) == 32);
    static_assert(sizeof(KyberTrailRaster) == 48);
    auto* trail = kyber_trail_create();
    assert(trail && kyber_trail_color(trail, "#4efade"));
    assert(!kyber_trail_color(trail, "invalid"));
    for (int i = 0; i < 50; ++i)
        kyber_trail_sample(trail, i * 0.01, 100 + i * 3, 200 + std::sin(i * .1) * 30);
    const auto bounds = kyber_trail_bounds(trail, .5);
    assert(bounds.width > 100 && bounds.height > 30);
    const auto image = kyber_trail_render(trail, .5, {0, 0, 1000, 800}, false, 1.0);
    assert(image.data && image.width > 100 && image.height > 30);
    bool visible = false;
    for (size_t i = 0; i < static_cast<size_t>(image.width) * image.height; ++i) {
        const auto* p = image.data + i * 4;
        assert(p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
        visible |= p[3] > 20;
    }
    assert(visible);
    assert(kyber_trail_points(trail) > 2);
    assert(kyber_trail_particles(trail) > 0);
    assert(kyber_trail_bounds(trail, 1.2).width == 0);
    assert(kyber_trail_points(trail) == 0);
    assert(kyber_trail_particles(trail) == 0);
    kyber_trail_clear(trail);
    kyber_trail_destroy(trail);
    std::cout << "C++/Rust ABI, premultiplied pixels, bounds, and fade cleanup passed\n";
}
