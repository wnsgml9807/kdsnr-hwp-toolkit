"""Frida raid 18 — capture cipher function pointer + input/output for type 0 fonts.

Hook FUN_100ad2c0 entry. Read param_9 (cipher callback). Hook the cipher and
log first cipher invocation's input/output bytes. Identifies which cipher
function is used for HJSMJ-style type 0 dispatches.
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

function installHooks(rawBase) {
    if (installed) return;
    installed = true;
    const dll = ptr(rawBase.toString());

    const A_AD2C0 = dll.add(0xAD2C0);
    send({t: "info", msg: "Attached at " + A_AD2C0.toString()});

    Interceptor.attach(A_AD2C0, {
        onEnter(args) {
            try {
                // ECX = param_1 (fs), EDX = param_2
                // args[i] are stack args starting from param_3
                this.fs = this.context.ecx;
                // Stack args: param_3 (char), param_4, param_5, param_6, param_7, param_8, param_9
                // arg index: args[0]=p3, [1]=p4, [2]=p5, [3]=p6, [4]=p7, [5]=p8, [6]=p9
                this.cipherCb = args[6];   // cipher function pointer
                this.char_code = args[0].toInt32() & 0xFFFF;

                // Read font filename for filtering
                this.fname = '';
                try {
                    this.fname = this.fs.add(0x04).readCString(32) || '';
                } catch (e) {}

                // Hook the cipher CB if not already
                if (!cipherHookedAt.has(this.cipherCb.toString())) {
                    cipherHookedAt.add(this.cipherCb.toString());
                    try {
                        Interceptor.attach(this.cipherCb, {
                            onEnter(cargs) {
                                this.buf = cargs[0];
                                this.len = cargs[1].toInt32();
                                this.before = this.buf.readByteArray(Math.min(this.len, 96));
                            },
                            onLeave(retval) {
                                const after = this.buf.readByteArray(Math.min(this.len, 96));
                                send({t: "cipher_call",
                                      cb_addr: '0x' + (this.cipherCb || 0).toString(16),
                                      len: this.len},
                                      this.before);
                                send({t: "cipher_after"}, after);
                            }
                        });
                        send({t: "cipher_cb_hooked",
                              addr: this.cipherCb.toString(),
                              offset_from_dll: '0x' + (this.cipherCb.toInt32() - rawBase).toString(16)});
                    } catch (e) {
                        send({t: "err", msg: 'cipher hook fail: ' + e});
                    }
                }
            } catch (e) {
                send({t: "err", msg: 'onEnter: ' + e});
            }
        },
        onLeave(retval) {
            // Per-call summary
            if (this.fname.toLowerCase().includes('hjsmj') ||
                this.fname.toLowerCase().includes('hchg')) {
                send({t: "ad2c0",
                      fname: this.fname,
                      char: '0x' + this.char_code.toString(16),
                      cipher_cb: this.cipherCb.toString()});
            }
        }
    });

    send({t: "info", msg: "FUN_100ad2c0 hook installed"});
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


cipher_data = {}


def on_msg(message, data):
    if message["type"] == "send":
        p = message["payload"]
        t = p.get("t")
        if t == "info":
            print(f">>> {p['msg']}", flush=True)
        elif t == "cipher_cb_hooked":
            print(f"\n*** CIPHER CB hooked at {p['addr']} (offset {p['offset_from_dll']} from HncBaseDraw.dll)", flush=True)
        elif t == "cipher_call":
            cipher_data["last_input"] = data
            cipher_data["last_cb"] = p["cb_addr"]
            cipher_data["last_len"] = p["len"]
            print(f"\n[cipher call] cb={p['cb_addr']}, len={p['len']}", flush=True)
            print(f"  input first 32: {data[:32].hex(' ') if data else ''}", flush=True)
        elif t == "cipher_after":
            inp = cipher_data.get("last_input")
            print(f"  output first 32: {data[:32].hex(' ') if data else ''}", flush=True)
            if inp and data:
                # Diff: XOR to see pattern
                diff = bytes(a ^ b for a, b in zip(inp[:32], data[:32]))
                print(f"  XOR diff: {diff.hex(' ')}", flush=True)
        elif t == "ad2c0":
            print(f"  [ad2c0] fname={p['fname']!r} char={p['char']} cipher_cb={p['cipher_cb']}", flush=True)
        elif t == "err":
            print(f">>> ERR: {p['msg']}", flush=True)


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
    print(">>> Hooked. In Hwp: change font to HJSMJ (한자 신명조) and type Hanja.", flush=True)
    print(">>> Or change font to HCHGGGT and type Korean to compare.", flush=True)
    print(">>> Ctrl+C to exit.", flush=True)
    try:
        while True:
            time.sleep(0.5)
    except KeyboardInterrupt:
        session.detach()


if __name__ == "__main__":
    main()
