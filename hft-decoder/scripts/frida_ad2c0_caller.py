"""Capture the caller of FUN_100ad2c0 (painter chain entry).

By snapshotting the return address on stack[0] when ad2c0 is entered, we
find the function that consumes the painter output buffer to actually
draw the glyph. That consumer is the real interpretation site of the
(marker, x, y) stream.
"""
import frida, sys, time, json


def find_hwp_pid():
    device = frida.get_local_device()
    for p in device.enumerate_processes():
        if p.name.lower() == "hwp.exe":
            return p.pid
    return None


SCRIPT = r"""
let installed = false;
let seen = new Set();

function installHooks(dllBase) {
    if (installed) return; installed = true;
    const dll = ptr(dllBase.toString());
    const m = Process.findModuleByName("HncBaseDraw.dll");
    const baseInt = m.base.toInt32();

    Interceptor.attach(dll.add(0xAD2C0), {
        onEnter(args) {
            // stack[0] = return address on x86 (after the call)
            const retAddr = this.context.esp.readPointer();
            const offset = retAddr.toInt32() - baseInt;
            const offStr = '0x' + (offset >>> 0).toString(16);
            // also capture full backtrace
            const bt = Thread.backtrace(this.context, Backtracer.ACCURATE)
                            .slice(0, 6)
                            .map(addr => {
                                const o = addr.toInt32() - baseInt;
                                return '0x' + (o >>> 0).toString(16);
                            });
            const key = offStr;
            if (seen.has(key)) return;
            seen.add(key);
            send({t:"caller", offset: offStr, ret_abs: retAddr.toString(), bt: bt});
        }
    });
    send({t:"info", msg:"ad2c0 caller-tracker armed."});
}

function tryInstall() {
    const m = Process.findModuleByName("HncBaseDraw.dll");
    if (m) { installHooks(m.base); return true; }
    return false;
}
if (!tryInstall()) { const i = setInterval(() => { if (tryInstall()) clearInterval(i); }, 200); }
"""


def on_msg(message, data):
    if message["type"] != "send":
        if message["type"] == "error":
            print(f">>> ERR: {message.get('description','?')}", flush=True)
        return
    p = message["payload"]
    if p.get("t") == "info":
        print(f">>> {p['msg']}", flush=True)
    else:
        print(json.dumps(p), flush=True)


def main():
    pid = find_hwp_pid()
    if pid is None:
        print("Hwp.exe not running"); sys.exit(1)
    print(f"Attaching to PID {pid}", flush=True)
    session = frida.attach(pid)
    script = session.create_script(SCRIPT)
    script.on("message", on_msg)
    script.load()
    print(">>> trigger render", flush=True)
    try:
        while True: time.sleep(0.5)
    except KeyboardInterrupt:
        session.detach()


if __name__ == "__main__":
    main()
