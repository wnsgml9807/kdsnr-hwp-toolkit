"""Frida GDI wrapper hook — dump CHncDuoDC::MoveToEx/LineTo/PolyBezierTo calls.

This is the *real* path-operator emission point in HncBaseDraw.dll. Once
glyph data has gone through the path painter (FUN_10029c50) and the
caller's interpretation logic, what actually ends up drawn is a sequence
of MoveTo / LineTo / PolyBezierTo calls — exactly the M / L / C operators
of SVG / PDF path.

By hooking these wrappers we get a 1:1 mapping for any glyph without
having to reverse-engineer the painter buffer interpretation ourselves.

Wrapper addresses (per pathcallers.txt decompile):
    PolyBezierTo  @ 0x1004c850   (HDC, tagPOINT*, count)
    MoveToEx      @ 0x1004c7b0   (HDC, x, y, tagPOINT*)
    LineTo        @ 0x1004c810   (HDC, x, y)

Each is __thiscall, so this=ECX. (HDC, x, y, ...) follow on stack.
"""
import frida
import sys
import time
import json


def find_hwp_pid():
    device = frida.get_local_device()
    for p in device.enumerate_processes():
        if p.name.lower() == "hwp.exe":
            return p.pid
    return None


SCRIPT = r"""
let installed = false;

// Track last (fname, char) context from ac080 hook.
let lastChar = '?';
let lastFname = '?';

function installHooks(dllBase) {
    if (installed) return;
    installed = true;
    const dll = ptr(dllBase.toString());

    // Context-tracking hooks (fname/char per glyph)
    Interceptor.attach(dll.add(0xAC080), {
        onEnter(args) {
            this.char = args[0].toInt32() & 0xFFFF;
            try { this.fname = this.context.ecx.add(0x04).readCString(20) || '?'; }
            catch (e) { this.fname = '?'; }
        },
        onLeave(retval) {
            lastChar = '0x' + this.char.toString(16);
            // strip non-printable garbage after .HFT
            const i = this.fname.indexOf('.HFT');
            lastFname = i > 0 ? this.fname.substring(0, i + 4) : this.fname;
        }
    });

    // CHncDuoDC::MoveToEx @ 0x4c7b0  — __thiscall(this, x, y, lpPoint)
    Interceptor.attach(dll.add(0x4C7B0), {
        onEnter(args) {
            const x = args[0].toInt32();
            const y = args[1].toInt32();
            send({t: "M", fname: lastFname, char: lastChar, x: x, y: y});
        }
    });

    // CHncDuoDC::LineTo @ 0x4c810 — __thiscall(this, x, y)
    Interceptor.attach(dll.add(0x4C810), {
        onEnter(args) {
            const x = args[0].toInt32();
            const y = args[1].toInt32();
            send({t: "L", fname: lastFname, char: lastChar, x: x, y: y});
        }
    });

    // CHncDuoDC::PolyBezierTo @ 0x4c850 — __thiscall(this, POINT*, count)
    Interceptor.attach(dll.add(0x4C850), {
        onEnter(args) {
            const pts_ptr = args[0];
            const count = args[1].toInt32();
            if (count <= 0 || count > 300 || pts_ptr.isNull()) return;
            const buf = pts_ptr.readByteArray(count * 8);
            send({t: "C", fname: lastFname, char: lastChar, count: count}, buf);
        }
    });

    send({t: "info", msg: "GDI wrapper hooks installed (MoveToEx/LineTo/PolyBezierTo)."});
}

function tryInstall() {
    const m = Process.findModuleByName("HncBaseDraw.dll");
    if (m) { installHooks(m.base); return true; }
    return false;
}

if (!tryInstall()) {
    const i = setInterval(() => { if (tryInstall()) clearInterval(i); }, 200);
}
"""


def on_msg(message, data):
    if message["type"] != "send":
        if message["type"] == "error":
            print(f">>> Frida ERROR: {message.get('description','?')}", flush=True)
        return
    p = message["payload"]
    t = p.get("t")
    if t == "info":
        print(f">>> {p['msg']}", flush=True)
        return
    rec = {"op": t, "fname": p["fname"], "char": p["char"]}
    if t in ("M", "L"):
        rec["x"] = p["x"]
        rec["y"] = p["y"]
    elif t == "C":
        count = p["count"]
        pts = []
        for i in range(count):
            x = int.from_bytes(data[i*8:i*8+4], "little", signed=True)
            y = int.from_bytes(data[i*8+4:i*8+8], "little", signed=True)
            pts.append([x, y])
        rec["count"] = count
        rec["pts"] = pts
    print(json.dumps(rec, ensure_ascii=False), flush=True)


def main():
    pid = find_hwp_pid()
    if pid is None:
        print("Hwp.exe not running")
        sys.exit(1)
    print(f"Attaching to PID {pid}", flush=True)
    session = frida.attach(pid)
    script = session.create_script(SCRIPT)
    script.on("message", on_msg)
    script.load()
    print(">>> GDI hooks installed. Trigger render in Hwp (scroll/print preview).", flush=True)
    print(">>> Ctrl+C when done", flush=True)
    try:
        while True:
            time.sleep(0.5)
    except KeyboardInterrupt:
        session.detach()


if __name__ == "__main__":
    main()
