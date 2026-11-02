# Third-party notices

Kyber Cursor's own code is MIT licensed (see [LICENSE](LICENSE)). It also
contains code adapted from the projects below. Their licenses apply to those
parts and are reproduced in full here.

## Plume

- Author: Andrei C. Popa ([qempexe](https://github.com/qempexe))
- Project: https://github.com/qempexe/plume
- License: MIT

The drag-trail looks in `rust/crates/kyber-core/src/trail.rs` (the Comet and
Sparks looks: their particle motion, sizes, timing and fading) are adapted from
Plume's `qml/js/trails/` scripts and rewritten in Rust. Kyber does not need
Plume installed and never calls it.

Thank you, Andrei, for Plume and for releasing it as free software.

```
MIT License

Copyright (c) 2026 Andrei C. Popa

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Worms cursor (otlet)

- Author: otlet
- Project: `otlet/omarchy-wormscursor` (the `otlet.worms-cursor` Omarchy plugin)

The Aim mode (the blade turns to face its direction of travel and keeps that
angle when the pointer stops) is that plugin's idea. The heading logic in
`rust/crates/kyber-core/src/aim.rs` (travel needed before a heading is taken,
heading smoothing, turn hysteresis and turn speed) is ported from its `Aim`
class and rewritten in Rust. The plugin's own code and artwork are not
included.

Thank you, otlet, for the Worms cursor.
