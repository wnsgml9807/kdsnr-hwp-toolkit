"""Frida raid 16 — capture FUN_100ac080 inputs and outputs.

Goal: validate the static decoder by capturing the exact (char_code → bitmap_indices)
mappings that Hwp produces at runtime.

Capture per call:
- args[0] = char_code (16-bit, first arg via ECX)
- arg param_6 (4th explicit arg) — pointer to local_2030[3] where bitmap indices written
- after the call: read local_2030[0..2] = bitmap indices used for cho/jung/jong
- arg param_5 (3rd explicit arg) — pointer to local_8 (component count, 0..3)
- font_struct field at +0x04 (= HFT filename), +0x60 (= font name) — sanity check

Run on Windows VM with Hwp.exe attached. Type 가, 나, 다, 라, 마, 잭, 한 etc. and observe.
"""
import frida
import sys
import time


def find_hwp_pid():
    device = frida.get_local_device()
    for p in device.enumerate_processes():
        if p.name.lower() == "hwp.exe":
            return p.pid
    return None


SCRIPT = r"""
let installed = false;

function installHooks(rawBase) {
    if (installed) return;
    installed = true;
    const dll = ptr(rawBase.toString());

    const A_AC080 = dll.add(0xAC080);
    send({t: "info", msg: "About to attach to " + A_AC080.toString()});

    Interceptor.attach(A_AC080, {
        onEnter(args) {
            // __fastcall: ECX = param_1 (fs), EDX = param_2 (chunk_meta)
            // Stack: param_3 (char), param_4, param_5, param_6, param_7, param_8
            this.fs = this.context.ecx;
            this.param_2 = this.context.edx;
            this.char = args[0].toInt32() & 0xFFFF;
            // param_5 = pointer to component_count (local_8 in caller)
            this.p5 = args[2];
            // param_6 = pointer to local_2030[3] (bitmap indices)
            this.p6 = args[3];
            // param_7 = local_2048[3] x_offsets
            this.p7 = args[4];
            // param_8 = local_2048[3..6] y_offsets
            this.p8 = args[5];
        },
        onLeave(retval) {
            try {
                // Sanity: font filename / name
                const fname = this.fs.add(0x04).readCString(32);
                const fontname = this.fs.add(0x60).readCString(32);

                // Component count
                const count = this.p5.readS32();

                // Bitmap indices (up to 3)
                const indices = [];
                for (let i = 0; i < 3; i++) {
                    indices.push(this.p6.add(i * 4).readU32());
                }
                // x/y offsets (for type 4)
                const x_offs = [];
                const y_offs = [];
                for (let i = 0; i < 3; i++) {
                    x_offs.push(this.p7.add(i * 4).readS32());
                    y_offs.push(this.p8.add(i * 4).readS32());
                }

                // Descriptor info from retval (= puVar17 runtime descriptor)
                if (!retval.isNull()) {
                    const flags = retval.add(0).readU16();
                    const range_start = retval.add(2).readU16();
                    const range_end = retval.add(4).readU16();
                    const desc_count = retval.add(6).readU16();
                    const em = retval.add(8).readU16();
                    const file_off = retval.add(0x14).readU32();
                    send({t: "ac080",
                          char: this.char,
                          fname: fname || '',
                          fontname: fontname || '',
                          count: count,
                          indices: indices,
                          x_offs: x_offs,
                          y_offs: y_offs,
                          flags: flags,
                          rs: range_start,
                          re: range_end,
                          desc_count: desc_count,
                          em: em,
                          file_off: file_off});
                } else {
                    send({t: "ac080_null", char: this.char});
                }
            } catch (e) {
                send({t: "err", char: this.char, msg: e.toString()});
            }
        }
    });

    send({t: "info", msg: "Hooked FUN_100ac080"});
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


# Track unique (char_code, fname) pairs to dedupe
seen = set()


def on_msg(message, data):
    if message["type"] == "send":
        p = message["payload"]
        t = p.get("t")
        if t == "info":
            print(f">>> {p.get('msg')}", flush=True)
        elif t == "ac080":
            key = (p["char"], p["fname"])
            if key in seen:
                return
            seen.add(key)
            char = p["char"]
            try:
                # Try to decode as Johab → Hangul Syllable
                if (char & 0x8000) and 0xA000 <= char <= 0xFFFF:
                    cho = (char >> 10) & 0x1f
                    jung = (char >> 5) & 0x1f
                    jong = char & 0x1f
                    decoded = f"johab cho={cho} jung={jung} jong={jong}"
                else:
                    decoded = ""
            except Exception:
                decoded = ""
            print(f"\n*** char=0x{char:04x} {decoded} ***", flush=True)
            print(f"  font: file='{p['fname']}', name='{p['fontname']}'", flush=True)
            print(f"  desc: type={p['flags'] & 0xf} (flags=0x{p['flags']:04x}), range=0x{p['rs']:x}..0x{p['re']:x}, cnt={p['desc_count']}, em={p['em']}, file_off=0x{p['file_off']:x}", flush=True)
            print(f"  result: count={p['count']}, indices={p['indices']}", flush=True)
            if any(x != 0 for x in p["x_offs"]) or any(y != 0 for y in p["y_offs"]):
                print(f"          x_offs={p['x_offs']}, y_offs={p['y_offs']}", flush=True)
        elif t == "ac080_null":
            print(f"  [null result for char=0x{p['char']:04x}]", flush=True)
        elif t == "err":
            print(f">>> ERR: char=0x{p['char']:04x} {p['msg']}", flush=True)
    elif message["type"] == "error":
        print(f">>> JS_ERROR: {message}", flush=True)


def main():
    pid = find_hwp_pid()
    if pid is None:
        print("Hwp.exe not running", flush=True)
        sys.exit(1)
    print(f"Attaching to PID {pid}", flush=True)
    session = frida.attach(pid)
    script = session.create_script(SCRIPT)
    script.on("message", on_msg)
    script.load()
    print(">>> Hooked. Now type Korean characters in Hwp (가, 나, 다, 라, 마, 잭, 한, etc.)", flush=True)
    print(">>> Each unique char will print once. Ctrl+C to exit.", flush=True)
    try:
        while True:
            time.sleep(0.5)
    except KeyboardInterrupt:
        session.detach()


if __name__ == "__main__":
    main()
