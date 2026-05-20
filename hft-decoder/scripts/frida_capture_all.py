"""Frida raid 18 (v2) — wide-net cipher + lookup capture for any HFT font.

Hooks ALL three glyph-rendering chains:
- FUN_100ac080: glyph lookup (called by every path)
- FUN_100ad2c0: vector painter chain (type 0/1/2 dispatchers)
- FUN_100acbf0: bitmap painter chain (with cipher)
- FUN_100ac550: bitmap painter chain (direct, no cipher)

Logs which one fires for each char_code so we can route Hanja correctly.
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
let cipherHookedAt = new Set();
let dllBase = null;

function installHooks(rawBase) {
    if (installed) return;
    installed = true;
    dllBase = rawBase;
    const dll = ptr(rawBase.toString());

    // Hook FUN_100ac080 (lookup — called by everything)
    Interceptor.attach(dll.add(0xAC080), {
        onEnter(args) {
            this.fs = this.context.ecx;
            this.char = args[0].toInt32() & 0xFFFF;
            this.fname = '';
            try { this.fname = this.fs.add(0x04).readCString(32) || ''; } catch(e) {}
        },
        onLeave(retval) {
            try {
                const flags = retval.isNull() ? 0 : retval.add(0).readU16();
                const tp = flags & 0xf;
                const bit4 = (flags >> 4) & 1;
                send({t: "ac080",
                      fname: this.fname,
                      char: '0x' + this.char.toString(16),
                      type: tp,
                      bit4: bit4,
                      retnull: retval.isNull()});
            } catch (e) {}
        }
    });

    // Hook FUN_100ad2c0 (vector path)
    Interceptor.attach(dll.add(0xAD2C0), {
        onEnter(args) {
            this.fs = this.context.ecx;
            this.cipherCb = args[6];
            this.char = args[0].toInt32() & 0xFFFF;
            this.fname = '';
            try { this.fname = this.fs.add(0x04).readCString(32) || ''; } catch(e) {}

            if (!cipherHookedAt.has(this.cipherCb.toString())) {
                cipherHookedAt.add(this.cipherCb.toString());
                try {
                    const cbAddr = this.cipherCb;
                    const offset = cbAddr.toInt32() - dllBase.toInt32();
                    Interceptor.attach(cbAddr, {
                        onEnter(cargs) {
                            this.buf = cargs[0];
                            this.len = cargs[1].toInt32();
                            this.before = this.buf.readByteArray(Math.min(this.len, 128));
                        },
                        onLeave(retval) {
                            const after = this.buf.readByteArray(Math.min(this.len, 128));
                            send({t: "cipher_call",
                                  cb: cbAddr.toString(),
                                  offset: '0x' + offset.toString(16),
                                  len: this.len,
                                  ret: retval.toInt32()},
                                  this.before);
                            send({t: "cipher_after"}, after);
                        }
                    });
                    send({t: "cipher_cb_hooked",
                          addr: cbAddr.toString(),
                          offset: '0x' + offset.toString(16)});
                } catch (e) {
                    send({t: "err", msg: 'cipher hook fail: ' + e});
                }
            }
        },
        onLeave(retval) {
            send({t: "ad2c0", fname: this.fname, char: '0x' + this.char.toString(16),
                  cipher_offset: '0x' + (this.cipherCb.toInt32() - dllBase.toInt32()).toString(16)});
        }
    });

    // Hook FUN_100acbf0 (bitmap path with cipher CB)
    Interceptor.attach(dll.add(0xACBF0), {
        onEnter(args) {
            this.fs = this.context.ecx;
            this.cipherCb = args[6];
            this.char = args[0].toInt32() & 0xFFFF;
            this.fname = '';
            try { this.fname = this.fs.add(0x04).readCString(32) || ''; } catch(e) {}
        },
        onLeave(retval) {
            send({t: "acbf0", fname: this.fname, char: '0x' + this.char.toString(16),
                  cipher_offset: '0x' + (this.cipherCb.toInt32() - dllBase.toInt32()).toString(16)});
        }
    });

    // Hook FUN_100ac550 (bitmap path no cipher)
    Interceptor.attach(dll.add(0xAC550), {
        onEnter(args) {
            this.fs = this.context.ecx;
            this.char = args[0].toInt32() & 0xFFFF;
            this.fname = '';
            try { this.fname = this.fs.add(0x04).readCString(32) || ''; } catch(e) {}
        },
        onLeave(retval) {
            send({t: "ac550", fname: this.fname, char: '0x' + this.char.toString(16)});
        }
    });

    send({t: "info", msg: "All 4 hooks installed. Type Korean+F9 for Hanja in Hwp."});
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


seen = {"ac080": set(), "ad2c0": set(), "acbf0": set(), "ac550": set()}
cipher_data = {}


def on_msg(message, data):
    if message["type"] == "send":
        p = message["payload"]
        t = p.get("t")
        if t == "info":
            print(f">>> {p['msg']}", flush=True)
        elif t == "cipher_cb_hooked":
            print(f"\n*** CIPHER CB hooked: {p['addr']} (offset {p['offset']})", flush=True)
        elif t == "cipher_call":
            cipher_data["last"] = (data, p)
            print(f"\n[cipher offset={p['offset']}, len={p['len']}, ret={p['ret']}]", flush=True)
            print(f"  in [:32]: {data[:32].hex(' ') if data else '∅'}", flush=True)
        elif t == "cipher_after":
            last = cipher_data.get("last")
            if last:
                inp, _meta = last
                print(f"  out[:32]: {data[:32].hex(' ') if data else '∅'}", flush=True)
                if inp and data and len(inp) >= 32 and len(data) >= 32:
                    diff = bytes(a ^ b for a, b in zip(inp[:32], data[:32]))
                    print(f"  XOR [:32]: {diff.hex(' ')}", flush=True)
        elif t in ("ac080", "ad2c0", "acbf0", "ac550"):
            key = (p["fname"], p["char"], p.get("type"), p.get("bit4"))
            if key in seen[t]:
                return
            seen[t].add(key)
            extra = []
            if "type" in p: extra.append(f"type={p['type']} bit4={p['bit4']}")
            if "cipher_offset" in p: extra.append(f"cipher={p['cipher_offset']}")
            extra_s = " " + " ".join(extra) if extra else ""
            print(f"  [{t}] fname={p['fname']!r} char={p['char']}{extra_s}", flush=True)
        elif t == "err":
            print(f">>> ERR: {p['msg']}", flush=True)


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
    print(">>> Hooked. Try in Hwp:", flush=True)
    print(">>>   • Type Korean (가나다)", flush=True)
    print(">>>   • Type Korean then F9 → Hanja (한국→韓國, 중국→中國)", flush=True)
    print(">>>   • Scroll, zoom — different size tiers", flush=True)
    print(">>>   Ctrl+C when done", flush=True)
    try:
        while True:
            time.sleep(0.5)
    except KeyboardInterrupt:
        session.detach()


if __name__ == "__main__":
    main()
