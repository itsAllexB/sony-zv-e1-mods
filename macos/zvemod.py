#!/usr/bin/env python3
"""ZV-E1 kit driver (macOS).

Self-contained driver: one process opens a service-mode session, runs one
operation, and leaves. Public subcommands:

  serve          (root) hold a service-mode session for ad-hoc commands
  exit           tell a running serve to leave service mode
  check          enter service mode, read a known-answer backup property,
                 leave. Read-only; nothing in RAM, flash or settings written.
  apply <level>  one-shot: enter service mode, push the matching helper to
                 the camera's RAM via the Jiritsu channel, collect its
                 report, leave. Supported levels:
                   70       card-speed bypass
                   76       temporal NR reduction
                   76check  read-only state check for level 76
                   76r      revert level 76

Everything the apply flow does is RAM-only. The kit never writes a backup
property, never calls setBackupData, never touches the camera's firmware or
settings. The jit-push guard refuses anything over 65520 bytes and refuses
any path outside kit/macos/helpers/.

Run from kit/macos/. Needs the .venv setup.command created and, for apply or
serve, sudo (macOS mass-storage driver detach requires root).
"""
from __future__ import annotations

import argparse
import contextlib
import datetime
import hashlib
import json
import os
import pathlib
import pwd
import socket
import struct
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
HELPERS_DIR = HERE / "helpers"
THIRD_PARTY = HERE / "third_party" / "sony-pmca-re"
RESULTS = HERE / "results"
sys.path.insert(0, str(THIRD_PARTY))

# Known-answer backup properties (pmca.platform.properties.PlatformBackupProps).
# These exist on every Sony service-mode camera and are safe reads.
BKID_MODEL_CODE = 0x00E70000
BKID_SERIAL = 0x00E70003

# Helpers allowed to be pushed, by level label -> filename. Any other label or
# any path outside helpers/ is refused.
HELPERS = {
    "70":       "zve1_level70_cardbypass.so",
    "76":       "zve1_level76_nr_disable.so",
    "76check":  "zve1_level76check.so",
    "76r":      "zve1_level76r_nr_revert.so",
}

HELPER_PLAIN_ENGLISH = {
    "70":  "Card-speed bypass is on. The camera will no longer refuse high-"
           "bitrate modes for the card's declared speed class.",
    "76":  "Less noise reduction is on. Unplug the camera, flick the "
           "Still/Movie dial stills -> movie -> stills -> movie, then record.",
    "76r": "Less noise reduction is off; the camera's stock temporal NR is "
           "back.",
    # The check helper has its own structured verdict (see _report_check).
}


# ---------------------------------------------------------------- Jiritsu / libJit channel
#
# These are the raw service-mode packets the owner's driver uses to deliver
# a small shared object over the camera's Jiritsu service-mode channel. Only
# the four Jiritsu file ops are implemented here; nothing in this file
# writes backup properties, and setBackupData is never called from any code
# path.

SENSER_HEADER = "<IHHBBBB"
PFUNC_ADJUST = 0x40
SET_FILE_PATH, GET_FILE2, SET_FILE2, FILE_DELETE2 = 0x400F, 0x4011, 0x4010, 0x4012
LIB_PATH = b"/tmp/jiritsu/libJit.so"
RESULT_PATH = b"../../tmp_wbshd/readcheck_result.txt"
LIB_MAX = 65520

STATUS_TEXT = {
    0x10000006: "no such file",
    0x10000000: "written, but the camera could not load it",
    0x10000011: "refused: /tmp/jbusy exists",
    0x1000000B: "remove failed",
    0x1000000F: "refused: service partition full",
    0x10000004: "service partition did not mount",
    0x10000008: "open for writing failed",
    0x10000009: "short write",
    0x10000015: "no transfer buffer",
    0x1000001C: "folder create failed",
    0x10000201: "request refused as malformed",
}


class OpError(Exception):
    """A refused or failed operation (reported, process stays sane)."""


def _owner() -> tuple[int, int]:
    """The real (non-root) uid/gid of whoever ran sudo."""
    return (
        int(os.environ.get("SUDO_UID", os.getuid())),
        int(os.environ.get("SUDO_GID", os.getgid())),
    )


def _sock_path() -> pathlib.Path:
    """macOS caps AF_UNIX paths at 104 bytes; keep it in the invoking user's home."""
    uid = int(os.environ.get("SUDO_UID", os.getuid()))
    return pathlib.Path(pwd.getpwuid(uid).pw_dir) / ".zve1" / "zvemod.sock"


SOCK = _sock_path()


def _read_service_answer(driver, sequence):
    """Read one service answer: (response_code, data).

    12-byte header, then up-to-1 MB parts read without ever asking for more
    than the current part still owes (some host USB stacks otherwise swallow
    padding).
    """
    def read_header():
        raw = b""
        for _ in range(4):
            raw = driver.read(struct.calcsize(SENSER_HEADER))
            if raw:
                break
        if len(raw) != struct.calcsize(SENSER_HEADER):
            raise OpError(f"service answer header is {len(raw)} bytes")
        return struct.unpack(SENSER_HEADER, raw)

    out = bytearray()
    total = None
    while True:
        size, _pfunc, seq, _v, _m, _o, response = read_header()
        if seq != sequence:
            raise OpError(
                f"service answer has sequence {seq}, expected {sequence}"
            )
        if total is None:
            total = size
        part = min(total - len(out), 0x100000)
        got = 0
        empty = 0
        while got < part:
            want = min(part - got, 0x8000)
            chunk = driver.read((want + 511) // 512 * 512)
            if not chunk:
                empty += 1
                if empty > 4:
                    raise OpError(
                        f"service answer stalled at {len(out) + got} of {total} bytes"
                    )
                continue
            empty = 0
            out += chunk
            got += len(chunk)
        if len(out) >= total:
            if total == 0:
                # Zero-length body is still followed by a 4-byte status word.
                out += driver.read(4)
            return response, bytes(out)


def _adjust(camera, code: int, payload: bytes) -> tuple[int, bytes]:
    """Send one 0x1001 adjust packet and read its answer (status word included)."""
    dev = camera.dev
    body = struct.pack("<HH", 0x1001, code) + payload
    header = struct.pack(
        SENSER_HEADER, len(body), PFUNC_ADJUST, dev._sequence, 0, 0, 0, 0
    )
    dev.driver.write(header + body)
    response, out = _read_service_answer(dev.driver, dev._sequence)
    dev._sequence += 1
    return response, out


def _select(camera, path: bytes) -> None:
    _adjust(camera, SET_FILE_PATH, b"\x00" + path)


def _unpack_status(answer: bytes) -> int:
    return struct.unpack_from("<I", answer)[0] if len(answer) >= 4 else -1


def op_jit_push(camera, local: str, remote: str) -> str:
    """Push a helper. Hard guards on size and on which local path is allowed."""
    remote_b = remote.encode()
    local_p = pathlib.Path(local).resolve()
    # Guard: only files inside the kit's helpers/ directory may be pushed.
    helpers_root = HELPERS_DIR.resolve()
    try:
        local_p.relative_to(helpers_root)
    except ValueError:
        raise OpError(
            f"refused: helper path {local_p} is outside {helpers_root}"
        )
    if not local_p.is_file():
        raise OpError(f"missing helper: {local_p}")
    data = local_p.read_bytes()
    if not 0 < len(data) <= LIB_MAX:
        raise OpError(f"refused: helper is {len(data)} bytes (1..{LIB_MAX})")
    _select(camera, remote_b)
    _resp, answer = _adjust(camera, SET_FILE2, b"\x00\xff\x00" + data)
    status = _unpack_status(answer)
    if status != 0:
        if status == -1:
            raise OpError(f"write gave no status ({len(answer)} bytes)")
        raise OpError(
            f"write refused: status 0x{status & 0xffffffff:08x} "
            f"({STATUS_TEXT.get(status, 'unknown')})"
        )
    return f"sent {len(data)} bytes to {remote} (status 0, written and loaded)"


def op_jit_pull(camera, remote: str, local: str) -> str:
    remote_b = remote.encode()
    _select(camera, remote_b)
    _resp, answer = _adjust(camera, GET_FILE2, b"\x00\xff\x00")
    status = _unpack_status(answer)
    if status != 0:
        if status == -1:
            raise OpError("read gave no status")
        raise OpError(
            f"read failed: status 0x{status & 0xffffffff:08x} "
            f"({STATUS_TEXT.get(status, 'unknown')})"
        )
    data = answer[4:]
    out = pathlib.Path(local)
    out.write_bytes(data)
    with contextlib.suppress(OSError):
        os.chown(out, *_owner())
    return f"read {len(data)} bytes from {remote} -> {out}"


def op_jit_rm(camera, remote: str) -> str:
    remote_b = remote.encode()
    _select(camera, remote_b)
    _resp, answer = _adjust(camera, FILE_DELETE2, b"\x00\xff")
    status = _unpack_status(answer)
    if status != 0:
        raise OpError(
            f"delete failed: status 0x{status & 0xffffffff:08x} "
            f"({STATUS_TEXT.get(status, 'unknown')})"
        )
    return f"deleted {remote}"


# ---------------------------------------------------------------- Camera link
#
# Link: enter service mode once, reconnect after a helper drops the camera off
# USB, never re-authenticate on a live session (that previously made the
# camera disappear from the bus).

class Link:
    def __init__(self, driver):
        self.driver = driver
        self.camera = None
        self.backend = None
        self.auth = None
        self.dead = True

    def connect(self, wait: float = 120.0) -> None:
        if self.dead:
            self.auth = None
        else:
            self.close()
        from pmca.commands.usb import listDevices
        from pmca.platform.backend.senser import SenserPlatformBackend
        from pmca.usb.driver.generic import GenericUsbDriver
        from pmca.usb.sony import (
            SonyMscExtCmdDevice,
            SonySenserAuthDevice,
            SonySenserCamera,
            SonySenserDevice,
        )

        end = time.time() + wait
        announced = False
        while time.time() < end:
            try:
                devs = list(listDevices(self.driver, True))
            except Exception:
                devs = []
            device = devs[0] if len(devs) == 1 else None
            if device is None:
                if not announced:
                    print("waiting for the camera to come back...", flush=True)
                    announced = True
                time.sleep(0.5)
                continue
            if isinstance(device, SonyMscExtCmdDevice):
                if not isinstance(device.driver, GenericUsbDriver):
                    raise SystemExit("libusb driver required for switching")
                print(
                    "camera is in mass storage; switching to service mode",
                    flush=True,
                )
                sw = SonySenserAuthDevice(device.driver)
                sw.start()
                sw.authenticate()
                time.sleep(0.5)
                continue
            if not isinstance(device, SonySenserDevice):
                time.sleep(0.5)
                continue
            print("service device found; authenticating", flush=True)
            try:
                auth = SonySenserAuthDevice(device.driver)
                auth.start()
                auth.authenticate()
            except Exception as exc:
                print(
                    f"  auth failed ({type(exc).__name__}); retrying",
                    flush=True,
                )
                time.sleep(1.0)
                continue
            camera = SonySenserCamera(device)
            backend = SenserPlatformBackend(camera)
            backend.start()
            self.camera, self.backend, self.auth, self.dead = (
                camera, backend, auth, False,
            )
            print("link up (settling)", flush=True)
            time.sleep(5.0)
            return
        raise OpError(
            f"the camera did not come back in {wait:g} s; cold-boot it "
            "and try again"
        )

    def close(self) -> None:
        if self.auth is not None:
            with contextlib.suppress(Exception):
                self.auth.stop()


@contextlib.contextmanager
def open_link():
    """libusb + authenticated service session; closes on exit."""
    from pmca.commands.usb import importDriver

    with importDriver("libusb") as driver:
        link = Link(driver)
        link.connect()
        try:
            yield link
        finally:
            print(
                "\nLeaving service mode (camera returns to mass storage).",
                flush=True,
            )
            link.close()


# ---------------------------------------------------------------- jit flow (one-shot)
#
# One apply = one helper: push, poll for the report, optionally remove the
# helper. Runs in-process, not socket-mediated.

def _jit_run_inproc(link: Link, helper_path: pathlib.Path, timeout: int) -> pathlib.Path:
    """Push the helper and return the local report path."""
    RESULTS.mkdir(parents=True, exist_ok=True)
    with contextlib.suppress(OSError):
        os.chown(RESULTS, *_owner())
    tmp = RESULTS / ".probe.tmp"

    # If a previous session left a stale libJit.so loaded, delete it (which
    # drops USB) and let the link reconnect before we push the new one.
    try:
        op_jit_pull(link.camera, LIB_PATH.decode(), str(tmp))
        print("a libJit.so is already on the camera; deleting it first",
              flush=True)
        with contextlib.suppress(OpError):
            op_jit_rm(link.camera, LIB_PATH.decode())
        tmp.unlink(missing_ok=True)
        time.sleep(8.0)
        link.dead = True
        link.connect()
    except OpError:
        pass

    # Clean up a stale report too, if one exists.
    try:
        op_jit_pull(link.camera, RESULT_PATH.decode(), str(tmp))
        stale = RESULTS / f"stale_{datetime.datetime.now():%Y%m%d_%H%M%S}.txt"
        with contextlib.suppress(OSError):
            tmp.replace(stale)
        print(f"a stale report was on the camera; saved to {stale}",
              flush=True)
        with contextlib.suppress(OpError):
            op_jit_rm(link.camera, RESULT_PATH.decode())
    except OpError:
        pass

    print(op_jit_push(link.camera, str(helper_path), LIB_PATH.decode()),
          flush=True)

    report = RESULTS / (
        f"zve1_{helper_path.stem}_{datetime.datetime.now():%Y%m%d_%H%M%S}.txt"
    )
    deadline = time.time() + timeout
    last = b""
    while time.time() < deadline:
        time.sleep(2)
        try:
            op_jit_pull(link.camera, RESULT_PATH.decode(), str(report))
        except OpError:
            continue
        except Exception:
            # USB hiccup: reconnect and keep polling.
            link.dead = True
            link.connect()
            continue
        got = report.read_bytes()
        if got and got == last and got.endswith(b"\n") and b"== " in got:
            break
        last = got
    else:
        raise OpError(
            f"no report after {timeout} s; the helper is still on the "
            "camera (a battery pull clears it)."
        )

    with contextlib.suppress(OpError, Exception):
        op_jit_rm(link.camera, RESULT_PATH.decode())
    return report


# ---------------------------------------------------------------- Top-level operations

def cmd_check() -> int:
    """Read-only session proof the camera is reachable."""
    with open_link() as link:
        backend = link.backend
        status = backend.getBackupStatus()
        region = status[0x14:].decode("latin1").rstrip("\x00")
        try:
            model = backend.readBackup(BKID_MODEL_CODE).decode("latin1").rstrip("\x00")
        except Exception as exc:
            print(f"model-code read failed: {type(exc).__name__}: {exc}",
                  flush=True)
            model = "?"
        print("")
        print("Service mode reachable: OK")
        print(f"  region:      {region or '(unknown)'}")
        print(f"  model code:  {model or '(unknown)'}")
        print("Read-only: no camera state was changed.")
    return 0


def _run_apply(level: str) -> int:
    helper_name = HELPERS[level]
    helper_path = HELPERS_DIR / helper_name
    helper_path = helper_path.resolve()
    # Double-check the allow-list hasn't been side-stepped.
    if helper_name not in HELPERS.values():
        raise OpError(f"refused: {helper_name} is not an allowed helper")
    try:
        helper_path.relative_to(HELPERS_DIR.resolve())
    except ValueError:
        raise OpError(
            f"refused: {helper_path} is not inside {HELPERS_DIR}"
        )
    if not helper_path.is_file():
        raise OpError(f"missing helper: {helper_path}")

    digest = hashlib.sha256(helper_path.read_bytes()).hexdigest()
    print(f"helper: {helper_path.name}  ({digest})", flush=True)

    with open_link() as link:
        report = _jit_run_inproc(link, helper_path, timeout=20)

    text = report.read_text(errors="replace")
    print("")
    print(f"report: {report} ({report.stat().st_size} bytes)")
    print("")
    # Show the first dozen lines of the helper's report, then the plain-English
    # outcome for this level.
    lines = text.splitlines()
    print("\n".join(lines[:12]))
    if len(lines) > 12:
        print(f"... {len(lines) - 12} more lines")
    print("")
    if level == "76check":
        _report_check(text)
    else:
        print(HELPER_PLAIN_ENGLISH[level])
        print("DONE.")
    return 0


def _report_check(text: str) -> None:
    """Parse the NR-reduction check helper's report and print the verdict."""
    word = None
    for line in text.splitlines():
        if "byte @" in line and "=" in line:
            word = line.split("=", 1)[1].strip()
    if word is None:
        print("Could not read the camera's state.")
        print("DONE.")
        return
    w = word.lower().removeprefix("0x")
    if w == "bf002000":
        print("Less noise reduction is CURRENTLY ON.")
    elif w == "f2d4f29e":
        print("Less noise reduction is OFF (camera at stock).")
    else:
        print("The patch site holds an unexpected value; the camera is in "
              "neither state this check knows.")
        print(f"  raw value read: 0x{w}")
    print("DONE.")


# ---------------------------------------------------------------- Serve/exit (optional)
#
# `apply` is the main user flow and does not use the socket. `serve` is kept
# for maintainers who want ad-hoc commands over a Unix socket.

def _serve_loop(link: Link) -> None:
    SOCK.parent.mkdir(mode=0o700, exist_ok=True)
    with contextlib.suppress(OSError):
        os.chown(SOCK.parent, *_owner())
    os.chmod(SOCK.parent, 0o700)
    if SOCK.exists():
        SOCK.unlink()
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(str(SOCK))
    with contextlib.suppress(OSError):
        os.chown(SOCK, *_owner())
    os.chmod(SOCK, 0o600)
    srv.listen(1)
    print(f"Service session open. Listening on {SOCK} (Ctrl-C to end).",
          flush=True)
    try:
        while True:
            conn, _ = srv.accept()
            with conn:
                req = json.loads(conn.makefile("r").readline() or "{}")
                cmd = req.get("cmd")
                args = req.get("args") or []
                try:
                    if cmd == "exit":
                        conn.sendall(
                            (json.dumps({"ok": True, "out": "leaving"}) + "\n").encode()
                        )
                        return
                    if link.dead:
                        link.connect()
                    out = _dispatch(link, cmd, args)
                    resp = {"ok": True, "out": out}
                except OpError as e:
                    resp = {"ok": False, "out": str(e)}
                except Exception as e:
                    link.dead = True
                    resp = {"ok": False, "out": f"{type(e).__name__}: {e}"}
                conn.sendall((json.dumps(resp) + "\n").encode())
    finally:
        srv.close()
        SOCK.unlink(missing_ok=True)


def _dispatch(link: Link, cmd: str, args: list) -> str:
    camera = link.camera
    if cmd == "status":
        return "service mode, session open"
    if cmd == "jit-push":
        return op_jit_push(camera, *args)
    if cmd == "jit-pull":
        return op_jit_pull(camera, *args)
    if cmd == "jit-rm":
        return op_jit_rm(camera, *args)
    raise OpError(f"unknown command {cmd!r}")


def cmd_serve() -> int:
    if os.geteuid() != 0:
        raise SystemExit("serve needs sudo (macOS mass-storage driver detach).")
    with open_link() as link:
        try:
            _serve_loop(link)
        except KeyboardInterrupt:
            pass
    return 0


def cmd_exit() -> int:
    if not SOCK.exists():
        print("no service session running")
        return 2
    c = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    c.settimeout(10)
    try:
        c.connect(str(SOCK))
    except (ConnectionRefusedError, FileNotFoundError):
        print("session socket is stale")
        return 2
    c.sendall((json.dumps({"cmd": "exit"}) + "\n").encode())
    resp = json.loads(c.makefile("r").readline())
    print(resp.get("out", ""))
    return 0 if resp.get("ok") else 1


# ---------------------------------------------------------------- CLI

def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("serve")
    sub.add_parser("exit")
    sub.add_parser("check")
    pa = sub.add_parser("apply")
    pa.add_argument("level", choices=sorted(HELPERS))
    pr = sub.add_parser("revert")
    pr.add_argument("level", choices=["76"], help="only 76 has a tested in-session revert")
    a = ap.parse_args()

    try:
        if a.cmd == "serve":
            return cmd_serve()
        if a.cmd == "exit":
            return cmd_exit()
        if a.cmd == "check":
            if os.geteuid() != 0:
                raise SystemExit(
                    "check needs sudo (macOS mass-storage driver detach)."
                )
            return cmd_check()
        if a.cmd == "apply":
            if os.geteuid() != 0:
                raise SystemExit(
                    "apply needs sudo (macOS mass-storage driver detach)."
                )
            return _run_apply(a.level)
        if a.cmd == "revert":
            if os.geteuid() != 0:
                raise SystemExit(
                    "revert needs sudo (macOS mass-storage driver detach)."
                )
            # revert 76 == apply 76r
            return _run_apply("76r")
    except OpError as e:
        print(f"ERROR: {e}", flush=True)
        return 1
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
