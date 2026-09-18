# M13 Design — Graphics, Input, KMS-like Contract, Repair IPC

> Status: design of record for v0.0.5, written before code (DESIGN §13.5).
> Roadmap: `ROADMAP_v0.0.2+.md` §5. This milestone freezes the interfaces
> that XFCE/Qt consume in M15; it does not ship a desktop.

## 1. Goal

A stable, documented platform surface for GUI software:

1. framebuffer + input APIs inside the kernel (drivers underneath),
2. a **KMS-like ioctl contract** that an unmodified Xorg modesetting driver
   can target from userspace (M14/M15),
3. a **repair-service IPC contract** that Qt frontends speak (M15),
4. a demo that proves the contracts without a userspace yet.

## 2. Framebuffer API (kernel)

- `struct fb_info { base, size, width, height, stride, format, bpp }`,
  sourced from GOP (x86), VBE (BIOS path) or virtio-gpu (later); the existing
  GOP console becomes one client.
- Operations: `blit(src, dst_rect)`, `fill(rect, pixel)`, `copy_rect`,
  `pan(offset)` and a **damage list** (bounded, coalesced rectangles).
- **Double buffering**: an off-screen buffer from the frame allocator when
  memory allows; otherwise direct rendering with damage-tracked updates.
  GOP has no flip, so `flip()` is a damage-list present; when a KMS-like
  device exists (M15 virtio-gpu), flip becomes a real page flip.
- Formats: start with the GOP `Bgrx8888`/`Bgra8888` set plus 16-bit
  fallback; conversion helpers for XRGB/BGR.

## 3. Input event API (kernel)

- `struct input_event { type, code, value }` (Linux evdev-compatible names
  and numbers so the userspace input stack needs no translation):
  `EV_KEY`, `EV_REL`, `EV_ABS`, `EV_SYN`.
- Sources: PS/2 keyboard (existing) + PS/2 mouse (new), virtio-input
  (MMIO/PCI) later; each source is an `input_ops` provider feeding one
  bounded ring buffer per device.
- Clients: the kernel console reads scancodes as today; in M14 the ring
  graduates to `/dev/input/event0` with `read()` returning event records.

## 4. KMS-like ioctl contract (frozen in this milestone)

The subset Xorg's `modesetting` driver needs, with Linux-compatible
numbers/structs where they are stable enough to copy field-for-field:

| ioctl | purpose |
|---|---|
| `DRM_IOCTL_VERSION` | driver identity |
| `DRM_IOCTL_MODE_GETRESOURCES` | CRTCs/connectors/encoders/FBs lists |
| `DRM_IOCTL_MODE_GETCONNECTOR` | connection state + modes + EDID blob |
| `DRM_IOCTL_MODE_GETCRTC` | current mode/framebuffer |
| `DRM_IOCTL_MODE_CREATEDUMB` + `MAP_DUMB` | dumb buffer alloc + mmap offset |
| `DRM_IOCTL_MODE_ADDFB` / `RMFB` | framebuffer objects |
| `DRM_IOCTL_MODE_SETCRTC` | mode set / page flip (with a flip event) |
| `DRM_IOCTL_MODE_PAGE_FLIP` | async flip + completion event |

- **EDID**: from GOP/VBE when available; otherwise a built-in 1024x768
  fallback blob, clearly marked in the connector properties.
- **Events**: a bounded event queue (flip completion, hotplug later)
  readable via `read()` on the DRM fd.
- The contract is documented in `GRAPHICS_API.md` (written in this
  milestone) with exact structs and semantics; breaking it later requires an
  append-only extension, never a renumber.

## 5. Repair-service IPC contract (frozen in this milestone)

- Transport: an `AF_UNIX`-style socket pair exposed by the kernel broker
  (M14 provides sockets; M13 defines and demos the protocol over an
  in-kernel pipe).
- Messages (little-endian, versioned header):
  `HELLO`/`HELLO_ACK`, `QUERY_DISKS`, `DIAGNOSE`, `REPAIR_PLAN`,
  `REPAIR_APPLY` (requires a confirmation token), `EVENTS` (progress),
  `CANCEL`, `BYE`.
- **Security**: the broker performs exactly the shell's operations through
  the same `RepairToken` gate; a frontend can never gain extra privilege.
  Cancel is honored between chunks (the imager and surface scan already
  poll a cancel flag).
- Documented in `REPAIR_IPC.md` with the message schema and the
  no-privilege-escalation statement.

## 6. Demo (proof of contract, no userspace)

- A kernel-side framebuffer demo (logo + live counters + a moving box)
  driven by the damage list, with a screendump diff in the smoke.
- A kernel-side IPC client that runs `QUERY_DISKS` + `DIAGNOSE` and prints
  the exchange, proving the protocol shape that Qt will speak.

## 7. Work breakdown (suggested commits)

| Step | Deliverable |
|---|---|
| M13-1 | `fb_info` + blit/fill/damage core; GOP console refactored onto it |
| M13-2 | Double buffer + present; demo app (kernel task) |
| M13-3 | Input event ring + PS/2 mouse; console + demo consumers |
| M13-4 | Dumb-buffer objects, ADDFB/SETCRTC/PAGE_FLIP semantics + events |
| M13-5 | EDID sourcing (GOP/VBE) + fallback blob + connector properties |
| M13-6 | Repair broker + protocol + in-kernel client demo |
| M13-7 | Freeze docs (`GRAPHICS_API.md`, `REPAIR_IPC.md`), smoke phases |

## 8. Verification

- Demo smoke: two GOP screendumps differ while the demo runs; damage
  accounting covers every changed pixel (checked in a test mode).
- Input: injected PS/2 events (existing QEMU monitor path) reach the ring
  and the demo reacts; no lost SYNs under a bounded burst.
- KMS contract: a struct-size/offset self-test compiled against the
  documented layout (catches accidental ABI drift in CI).
- IPC: the in-kernel client completes HELLO/QUERY_DISKS/DIAGNOSE and a
  cancelled REPAIR_APPLY leaves nothing written (negative check).

## 9. Risks

| Risk | Mitigation |
|---|---|
| Xorg modesetting needs more than the subset | the table above is the documented minimum; additions are appended with tests before M15 |
| Struct ABI drift | self-test on sizes/offsets in CI; docs are the source of truth |
| Wrong EDID breaks mode selection | built-in fallback plus explicit "synthetic EDID" property |
| IPC misuse escalates privilege | broker uses the same gate as the shell; stated in REPAIR_IPC.md |
| Memory for double buffering | fall back to direct + damage when the frame allocator cannot spare a full buffer |
