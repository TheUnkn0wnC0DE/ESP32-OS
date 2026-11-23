# spatz

A small software 3D engine running entirely on the CPU: rasterizer, math, soft-body physics, animation and PNG output. The core is `no_std` with `alloc`, so the console games use it on the ESP32-P4. Licensed MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`).

Test (in the repository root): `cargo test --release -p spatz`.
