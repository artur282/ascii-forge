# figlet-rs (vendored)

This directory vendors `figlet-rs` 1.0.0 by yuanbohan, licensed under
Apache-2.0 (see `LICENSE-APACHE` and `COPYRIGHT`).

ASCII Forge only uses the FIGlet path and the four built-in `.flf` fonts. The
upstream crate depends on `zip` 0.6 for Toilet `.tlf` archives; that version has
a critical parser vulnerability. This copy removes the unused Toilet support and
the `zip` dependency entirely, and explicitly rejects zip-packaged fonts.

Upstream: https://github.com/yuanbohan/rs-figlet
