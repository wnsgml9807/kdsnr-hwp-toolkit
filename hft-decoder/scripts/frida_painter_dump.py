"""Frida painter buffer dump — A2 dynamic noseon.

Hooks FUN_10029c50 (path painter) at HncBaseDraw.dll +0x29c50.

Per path_painter.txt:
  int __fastcall FUN_10029c50(int *param_1, int param_2, int param_3,
                              int *param_4, byte *param_5, int param_6, int param_7)

  - ECX = param_1: pointer to struct {em, m1, m2, raw_bytes_ptr}
  - EDX = param_2: scale1
  - stack[0] = param_3: scale2
  - stack[1] = param_4: coord buffer (each entry = 2 ints x, y)        ★
  - stack[2] = param_5: marker buffer (each entry = 1 byte: 1/2/4)     ★
  - stack[3] = param_6: initial x
  - stack[4] = param_7: initial y
  - return  = local_c = number of points emitted

For each painter call we capture:
  - fname (from font_struct +0x04, like other hooks)
  - char code (from ECX side struct or recent ac080 cache)
  - raw bytes (param_1[3], length unknown so we read up to 2KB)
  - count = retval
  - coords = read(param_4, count * 8)
  - markers = read(param_5, count)

Usage:
    python frida_painter_dump.py
Then in Hwp.exe: type Korean / open document / trigger render.
Output goes to stdout one event per painter call.
"""
import frida
import sys
import time
import struct


def find_hwp_pid():
    device = frida.get_local_device()
    for p in device.enumerate_processes():
        if p.name.lower() == "hwp.exe":
            return p.pid
    return None


SCRIPT = r"""
let installed = false;

// last lookup char per fname (from ac080)
let lastChar = '?';
let lastFname = '?';

// Dedupe by (fname, char) — emit one painter dump per unique pair.
let seen = new Set();

function installHooks(dllBase) {
    if (installed) return;
    installed = true;
    const dll = ptr(dllBase.toString());

    // Hook ac080 to capture (fname, char) context just before painter fires.
    Interceptor.attach(dll.add(0xAC080), {
        onEnter(args) {
            this.char = args[0].toInt32() & 0xFFFF;
            try {
                this.fname = this.context.ecx.add(0x04).readCString(20) || '?';
            } catch (e) { this.fname = '?'; }
        },
        onLeave(retval) {
            lastChar = '0x' + this.char.toString(16);
            lastFname = this.fname;
        }
    });

    // Hook painter
    Interceptor.attach(dll.add(0x29C50), {
        onEnter(args) {
            this.param1 = this.context.ecx;
            this.coord_buf = args[1];
            this.marker_buf = args[2];
            this.x0 = args[3].toInt32();
            this.y0 = args[4].toInt32();
            this.char = lastChar;
            this.fname = lastFname;
            // Read param_1 struct: [em, m1, m2, raw_ptr] = 16 bytes
            try {
                this.p1struct = this.param1.readByteArray(16);
                this.raw_ptr = this.param1.add(12).readPointer();
            } catch (e) {
                this.p1struct = null;
                this.raw_ptr = ptr(0);
            }
        },
        onLeave(retval) {
            const count = retval.toInt32();
            if (count <= 0 || count > 4000) return;
            if (this.coord_buf.isNull() || this.marker_buf.isNull()) return;
            const key = this.fname + '|' + this.char;
            if (seen.has(key)) return;
            seen.add(key);
            let coords = null;
            let markers = null;
            let rawhead = null;
            try {
                coords = this.coord_buf.readByteArray(count * 8);
                markers = this.marker_buf.readByteArray(count);
                if (!this.raw_ptr.isNull()) {
                    rawhead = this.raw_ptr.readByteArray(64);  // first 64 bytes of raw
                }
            } catch (e) {
                send({t: "err", msg: 'buf read fail: ' + e});
                return;
            }
            send({t: "painter",
                  fname: this.fname,
                  char: this.char,
                  count: count,
                  x0: this.x0, y0: this.y0,
                  p1struct_b64: null});
            send({t: "coords", count: count}, coords);
            send({t: "markers", count: count}, markers);
            if (rawhead) send({t: "rawhead"}, rawhead);
        }
    });

    send({t: "info", msg: "Painter hook installed (FUN_10029c50)."});
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


# State for paired send messages (painter + coords + markers + rawhead come as 4 messages)
pending = {}


def on_msg(message, data):
    if message["type"] != "send":
        if message["type"] == "error":
            print(f">>> Frida ERROR: {message.get('description','?')}", flush=True)
        return
    p = message["payload"]
    t = p.get("t")
    if t == "info":
        print(f">>> {p['msg']}", flush=True)
    elif t == "err":
        print(f">>> ERR: {p['msg']}", flush=True)
    elif t == "painter":
        pending.clear()
        pending["meta"] = p
    elif t == "coords":
        pending["coords"] = data
    elif t == "markers":
        pending["markers"] = data
    elif t == "rawhead":
        pending["rawhead"] = data
        # All four arrived → emit one consolidated line
        emit_event()


def emit_event():
    import json
    m = pending.get("meta")
    coords = pending.get("coords")
    markers = pending.get("markers")
    if not m or coords is None or markers is None:
        return
    count = m["count"]
    pts = []
    for i in range(count):
        x = int.from_bytes(coords[i*8:i*8+4], "little", signed=True)
        y = int.from_bytes(coords[i*8+4:i*8+8], "little", signed=True)
        mk = markers[i]
        pts.append([mk, x, y])
    rawhead = pending.get("rawhead", b'')
    # Strip the fname garbage past .HFT — keep only up to first non-printable after .HFT
    fname_raw = m['fname']
    if '.HFT' in fname_raw:
        fname = fname_raw[: fname_raw.index('.HFT') + 4]
    else:
        fname = fname_raw.split('\x00')[0]
    rec = {
        "fname": fname,
        "char": m['char'],
        "count": count,
        "x0": m['x0'], "y0": m['y0'],
        "raw_hex": rawhead.hex() if rawhead else '',
        "points": pts,
    }
    print(json.dumps(rec, ensure_ascii=False), flush=True)
    pending.clear()


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
    print(">>> Type Korean in Hwp / open document / trigger render.", flush=True)
    print(">>> Ctrl+C when done", flush=True)
    try:
        while True:
            time.sleep(0.5)
    except KeyboardInterrupt:
        session.detach()


if __name__ == "__main__":
    main()
