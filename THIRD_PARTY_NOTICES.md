# Third-party notices

Nexus incorporates open-source software. This file is a **practical inventory**
of the main licenses that affect distribution. It is not an exhaustive legal
opinion. Versions are those resolved by this tree at the time of writing;
rebuild with `go list -m` / `cargo metadata` for exact pins.

Full license texts:

| License | Path |
|---------|------|
| GPLv3 | [`licenses/GPL-3.0.txt`](licenses/GPL-3.0.txt) |
| MPL-2.0 | [`licenses/MPL-2.0.txt`](licenses/MPL-2.0.txt) |

## How Nexus uses them

| Component | Role | Link model |
|-----------|------|------------|
| **NexusCore** (`core/server`) | Proxy engine process | Go packages are **compiled into** `NexusCore` |
| **Nexus shell** (`app/`) | Qt Quick host + Rust C ABI | Talks to Core over framed IPC; Rust crates compile into the library the Qt host links |

Distributing `NexusCore` (alone or inside `Nexus.app`) triggers
**GPLv3** obligations for that Core binary, including Corresponding Source.
See root [`LICENSE`](LICENSE) and [`core/server/LICENSE`](core/server/LICENSE).

---

## A. NexusCore — copyleft / strong obligations

### GPLv3 (and GPL family)

These modules are (or pull) GPLv3-licensed code used by Core. Non-exhaustive:

| Module | License | Notes |
|--------|---------|--------|
| `github.com/sagernet/sing-box` (replace: `github.com/Throneproj/sing-box`) | GPLv3 | Primary engine |
| `github.com/sagernet/sing` | GPLv3 | |
| `github.com/sagernet/sing-tun` | GPLv3 | TUN stack |
| `github.com/sagernet/sing-mux` | GPLv3 | transitive |
| `github.com/sagernet/sing-quic` | GPLv3 | transitive |
| `github.com/sagernet/sing-shadowsocks` / `sing-shadowsocks2` | GPLv3 | transitive |
| `github.com/sagernet/sing-shadowtls` | GPLv3 | transitive |
| `github.com/sagernet/sing-vmess` | GPLv3 | transitive |
| `github.com/sagernet/fswatch` | GPLv3 | transitive |
| `github.com/sagernet/cronet-go` (+ `lib/*`) | GPLv3 | naive/cronet path |
| `github.com/anytls/sing-anytls` | GPLv3 | transitive |
| `github.com/dyhkwong/sing-juicity` | GPLv3 | transitive |
| `github.com/enfein/mieru/v3` | GPLv3 | transitive |
| `github.com/xchacha20-poly1305/sing-trusttunnel` | GPLv3 | transitive when present |

Upstream project sites (examples):  
https://github.com/SagerNet/sing-box · https://github.com/Throneproj/sing-box

### MPL-2.0

| Module | License | Notes |
|--------|---------|--------|
| `github.com/xtls/reality` | MPL-2.0 | transitive through the sing-box graph |
| `github.com/hashicorp/yamux` | MPL-2.0 | transitive when present |

Nexus does not bundle Xray Core.

### LGPL

| Module | License | Notes |
|--------|---------|--------|
| `github.com/juju/ratelimit` | LGPLv3 | transitive; dynamic-link style obligations differ—still notice it |

---

## B. NexusCore — permissive direct / common deps (summary)

Typical licenses: **MIT**, **Apache-2.0**, **BSD**. Examples among direct requires:

| Module | License (as packaged) |
|--------|------------------------|
| `github.com/spf13/cobra` | Apache-2.0 |
| `google.golang.org/grpc` | Apache-2.0 |
| `google.golang.org/protobuf` | BSD-3-Clause |
| `golang.org/x/crypto` / `golang.org/x/sys` | BSD-3-Clause |
| `github.com/tailscale/go-winio` | MIT |
| `github.com/Mahdi-zarei/speedtest-go` | MIT |
| `github.com/dustin/go-humanize` | MIT |
| `github.com/gofrs/uuid` | MIT |
| `github.com/google/shlex` | Apache-2.0 |
| `github.com/sagernet/gvisor` | Apache-2.0 |
| `github.com/sagernet/wireguard-go` / `golang.zx2c4.com/wireguard` | MIT |
| `golang.zx2c4.com/wintun` | MIT |
| `github.com/ebitengine/purego` | Apache-2.0 |
| `github.com/cloudflare/circl` | BSD-3-Clause |
| `github.com/miekg/dns` | BSD-3-Clause |
| `github.com/gorilla/websocket` | BSD-2-Clause |
| `github.com/cretz/bine` | MIT |
| `github.com/refraction-networking/utls` / `github.com/metacubex/utls` | BSD-3-Clause |

Many further **indirect** modules are MIT/Apache/BSD. Generate a full list with:

```bash
cd core/server && go list -m all
# optional: go install github.com/google/go-licenses@latest && go-licenses report .
```

---

## C. Nexus shell (Rust + Qt)

Direct runtime-oriented crates (from `app/src-tauri/Cargo.toml`) are generally
**MIT and/or Apache-2.0** (Tauri 2, serde, prost, qrcode, socket2, etc.).

Copyleft among the wider Cargo graph (usually weak / file-level):

| Crate | License |
|-------|---------|
| `cssparser`, `selectors`, `cssparser-macros`, `dtoa-short`, `option-ext` | MPL-2.0 |
| `r-efi` (and similar dual-licensed) | MIT OR Apache-2.0 OR LGPL-2.1-or-later |

Full graph:

```bash
cd app/src-tauri && cargo metadata --format-version 1 --locked
```

Product GUI is Qt Quick (`app/qt`):

| Component | License | Notes |
|-----------|---------|--------|
| Qt 6.11 (`Quick`, `QuickControls2`, `Widgets`, `Svg`, `Gui`) | LGPLv3 | Dynamically linked. https://www.qt.io/licensing |
| `quirc` | ISC-style | QR image decoder; source is vendored under `app/qt/third_party/quirc` |

The Tauri crate remains as a compile dependency of the Rust library the Qt host links; it is not the window. Former HTML lives in `archive/webview-ui/`.

### quirc license

Copyright (C) 2010-2012 Daniel Beer <dlbeer@gmail.com>

Permission to use, copy, modify, and/or distribute this software for any purpose
with or without fee is hereby granted, provided that the above copyright notice
and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND
FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS
OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER
TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF
THIS SOFTWARE.

---

## D. What to ship with binaries (Plan A checklist)

When you give someone `Nexus.app` or `NexusCore`:

1. This file (`THIRD_PARTY_NOTICES.md`) or an equivalent notice.
2. `licenses/GPL-3.0.txt` and `licenses/MPL-2.0.txt` (or URLs plus offer).
3. **Corresponding Source** for the exact NexusCore you built (git tag/commit,
   `core/server`, root `build.sh` / documented build steps, and any applied
   patches such as the darwin ProcessID patch).
4. Do **not** claim the entire product is closed proprietary if Core is included.

---

## E. Trademarks / naming

Upstream projects may restrict use of their names/logos. Nexus does not claim
affiliation with SagerNet, XTLS, Throne, or other upstream brands beyond
factual dependency attribution.
