"""Probe HncBaseDraw.dll for exported outline-related APIs.

Goal: find HncGetGlyphOutline, _HncDRGetOutlinePts, and any other
"Glyph"/"Outline"/"Pts" symbols that we can call directly from Frida
to extract glyph outline points without going through the painter buffer.

Strategy:
  1. Enumerate exports of HncBaseDraw.dll (and related dlls).
  2. List names matching outline-related patterns.
  3. Print address + name so we can pick targets for NativeFunction call.
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
function dumpModule(modName) {
    const m = Process.findModuleByName(modName);
    if (!m) {
        send({t: "warn", msg: modName + " not loaded"});
        return;
    }
    const exports = m.enumerateExports();
    send({t: "module", name: modName, base: m.base.toString(), nexp: exports.length});
    const interesting = exports.filter(e => /outline|glyph|pts|char|drget|hncgg|font|drchar/i.test(e.name));
    for (const e of interesting) {
        send({t: "exp", mod: modName, name: e.name, addr: e.address.toString(),
              offset: '0x' + (e.address.toInt32() - m.base.toInt32()).toString(16)});
    }
}

const targets = ["HncBaseDraw.dll", "HncFontLib.dll", "HncHftExtp.dll", "HncBase.dll", "HncTextEngine.dll"];
for (const t of targets) dumpModule(t);
send({t: "done"});
"""


def on_msg(message, data):
    if message["type"] != "send":
        return
    p = message["payload"]
    t = p.get("t")
    if t == "warn":
        print(f"[WARN] {p['msg']}", flush=True)
    elif t == "module":
        print(f"\n=== {p['name']} @ {p['base']} ({p['nexp']} exports) ===", flush=True)
    elif t == "exp":
        print(f"  {p['offset']:>8}  {p['name']}", flush=True)
    elif t == "done":
        print("\nDONE", flush=True)


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
    time.sleep(2)
    session.detach()


if __name__ == "__main__":
    main()
