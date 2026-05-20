"""Hook ?GetGlyphOutlineW@CHncDuoDC at 0x29e0 in HncBaseDraw.dll.

Win32 standard signature:
  DWORD GetGlyphOutlineW(HDC hdc, UINT uChar, UINT uFormat,
                         LPGLYPHMETRICS lpgm, DWORD cbBuffer,
                         LPVOID lpvBuffer, const MAT2* lpmat2);

This wrapper is __thiscall, so ECX = this and (uChar, uFormat, lpgm,
cbBuffer, lpvBuffer, lpmat2) follow on the stack.

If Hwp calls this naturally during PDF export or print preview, we capture
lpvBuffer (the TTPOLYGONHEADER + TTPOLYCURVE sequence) for every glyph
without having to fake an HDC.
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
let lastChar = '?';
let lastFname = '?';

function installHooks(dllBase) {
    if (installed) return;
    installed = true;
    const dll = ptr(dllBase.toString());

    // ac080 context (fname, char)
    Interceptor.attach(dll.add(0xAC080), {
        onEnter(args) {
            this.char = args[0].toInt32() & 0xFFFF;
            try { this.fname = this.context.ecx.add(0x04).readCString(20) || '?'; }
            catch (e) { this.fname = '?'; }
        },
        onLeave(retval) {
            lastChar = '0x' + this.char.toString(16);
            const i = this.fname.indexOf('.HFT');
            lastFname = i > 0 ? this.fname.substring(0, i + 4) : this.fname;
        }
    });

    // ?GetGlyphOutlineW@CHncDuoDC @ 0x29e0
    // __thiscall(this, uChar, uFormat, lpgm, cbBuffer, lpvBuffer, lpmat2)
    Interceptor.attach(dll.add(0x29E0), {
        onEnter(args) {
            this.uChar = args[0].toInt32() & 0xFFFFFFFF;
            this.uFormat = args[1].toInt32();
            this.lpgm = args[2];
            this.cbBuf = args[3].toInt32();
            this.lpBuf = args[4];
            this.fname = lastFname;
            send({t: "ggo_enter",
                  fname: lastFname,
                  uChar: '0x' + this.uChar.toString(16),
                  uFormat: this.uFormat,
                  cbBuf: this.cbBuf,
                  lpBuf_null: this.lpBuf.isNull()});
        },
        onLeave(retval) {
            const size = retval.toInt32();
            if (size > 0 && size < 65536 && !this.lpBuf.isNull() && this.cbBuf >= size) {
                try {
                    const data = this.lpBuf.readByteArray(size);
                    send({t: "ggo_buf", fname: this.fname,
                          uChar: '0x' + this.uChar.toString(16),
                          uFormat: this.uFormat,
                          size: size}, data);
                } catch (e) {
                    send({t: "err", msg: 'buf read fail: ' + e});
                }
            } else {
                send({t: "ggo_ret",
                      fname: this.fname,
                      uChar: '0x' + this.uChar.toString(16),
                      uFormat: this.uFormat,
                      size: size, cb: this.cbBuf});
            }
        }
    });

    send({t: "info", msg: "GetGlyphOutlineW hook installed (CHncDuoDC@0x29e0)."});
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


pending = {}


def on_msg(message, data):
    if message["type"] != "send":
        if message["type"] == "error":
            print(f">>> ERR: {message.get('description','?')}", flush=True)
        return
    p = message["payload"]
    t = p.get("t")
    if t == "info":
        print(f">>> {p['msg']}", flush=True)
    elif t == "err":
        print(f">>> {p['msg']}", flush=True)
    elif t == "ggo_enter":
        rec = {"phase": "enter", **p}
        print(json.dumps(rec), flush=True)
    elif t == "ggo_buf":
        rec = {"phase": "buf",
               "fname": p["fname"], "uChar": p["uChar"], "uFormat": p["uFormat"],
               "size": p["size"], "hex": data.hex()}
        print(json.dumps(rec), flush=True)
    elif t == "ggo_ret":
        rec = {"phase": "ret", **p}
        print(json.dumps(rec), flush=True)


def main():
    pid = find_hwp_pid()
    if pid is None:
        print("Hwp.exe not running"); sys.exit(1)
    print(f"Attaching to PID {pid}", flush=True)
    session = frida.attach(pid)
    script = session.create_script(SCRIPT)
    script.on("message", on_msg)
    script.load()
    print(">>> Trigger render (scroll/print preview/PDF export).", flush=True)
    try:
        while True: time.sleep(0.5)
    except KeyboardInterrupt:
        session.detach()


if __name__ == "__main__":
    main()
