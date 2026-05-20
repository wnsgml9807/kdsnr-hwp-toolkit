"""Wide-net hook of all outline-related entry points in HncBaseDraw.dll.

Targets (from export probe):
  0x29e0   ?GetGlyphOutlineW@CHncDuoDC@@UBEKIIPAU_GLYPHMETRICS@@KPAXPBU_MAT2@@@Z
  0x2e2c0  ?GetGlyphOutlineW@CHncDeviceContext@@UBEKIIPAU_GLYPHMETRICS@@KPAXPBU_MAT2@@@Z
  0x2f1a0  ?GetGlyphOutlineW@@YGKPAVCHncDeviceContext@@IIPAU_GLYPHMETRICS@@KPAXPBU_MAT2@@@Z
  0x74f60  HncGetGlyphOutline
  0x74e60  FUN_10074e60 (internal of HncGetGlyphOutline)
  0x18b10  HncDRGetBezierCurve
  0x18bb0  _HncDRGetOutlinePts (internal symbol)
  0xa9d10  FUN_100a9d10 (forwarded from _HncDRGetOutlinePts)

Goal: find out which one(s) actually fire during natural Hwp render
(scroll / print preview / PDF export) and capture their args + buffer.
"""
import frida
import sys
import time
import json


TARGETS = [
    (0x29E0,   "GetGlyphOutlineW@CHncDuoDC"),
    (0x2E2C0,  "GetGlyphOutlineW@CHncDeviceContext"),
    (0x2F1A0,  "GetGlyphOutlineW@global"),
    (0x74F60,  "HncGetGlyphOutline"),
    (0x74E60,  "FUN_10074e60"),
    (0x18B10,  "HncDRGetBezierCurve"),
    (0x18BB0,  "_HncDRGetOutlinePts"),
    (0xA9D10,  "FUN_100a9d10"),
]


def find_hwp_pid():
    device = frida.get_local_device()
    for p in device.enumerate_processes():
        if p.name.lower() == "hwp.exe":
            return p.pid
    return None


# JS template — we expand TARGETS into JS inline
JS_TARGETS = ',\n'.join(f'    {{ off: 0x{addr:x}, name: "{name}" }}' for addr, name in TARGETS)

SCRIPT = (
    "let installed = false;\n"
    "const TARGETS = [\n"
    + JS_TARGETS + "\n"
    "];\n"
    "let lastChar = '?';\n"
    "let lastFname = '?';\n"
    "let seen = {};\n"
    "function installHooks(dllBase) {\n"
    "    if (installed) return; installed = true;\n"
    "    const dll = ptr(dllBase.toString());\n"
    "    // ac080 context\n"
    "    Interceptor.attach(dll.add(0xAC080), {\n"
    "        onEnter(args) {\n"
    "            this.char = args[0].toInt32() & 0xFFFF;\n"
    "            try { this.fname = this.context.ecx.add(0x04).readCString(20) || '?'; }\n"
    "            catch (e) { this.fname = '?'; }\n"
    "        },\n"
    "        onLeave(retval) {\n"
    "            lastChar = '0x' + this.char.toString(16);\n"
    "            const i = this.fname.indexOf('.HFT');\n"
    "            lastFname = i > 0 ? this.fname.substring(0, i + 4) : this.fname;\n"
    "        }\n"
    "    });\n"
    "    for (const t of TARGETS) {\n"
    "        const addr = dll.add(t.off);\n"
    "        const name = t.name;\n"
    "        Interceptor.attach(addr, {\n"
    "            onEnter(args) {\n"
    "                this.tname = name;\n"
    "                this.a0 = args[0];\n"
    "                this.a1 = args[1];\n"
    "                this.a2 = args[2];\n"
    "                this.a3 = args[3];\n"
    "                this.a4 = args[4];\n"
    "                this.a5 = args[5];\n"
    "                this.ecx = this.context.ecx;\n"
    "                send({t:'enter', name:name, fname:lastFname, char:lastChar,\n"
    "                      ecx:this.ecx.toString(),\n"
    "                      a0:this.a0.toString(), a1:this.a1.toString(),\n"
    "                      a2:this.a2.toString(), a3:this.a3.toString(),\n"
    "                      a4:this.a4.toString(), a5:this.a5.toString()});\n"
    "            },\n"
    "            onLeave(retval) {\n"
    "                send({t:'leave', name:name, ret:retval.toString(),\n"
    "                      retInt:retval.toInt32()});\n"
    "            }\n"
    "        });\n"
    "    }\n"
    "    send({t:'info', msg:'Wide-net outline hooks installed (' + TARGETS.length + ').'});\n"
    "}\n"
    "function tryInstall() {\n"
    "    const m = Process.findModuleByName('HncBaseDraw.dll');\n"
    "    if (m) { installHooks(m.base); return true; }\n"
    "    return false;\n"
    "}\n"
    "if (!tryInstall()) { const i = setInterval(() => { if (tryInstall()) clearInterval(i); }, 200); }\n"
)


def on_msg(message, data):
    if message["type"] != "send":
        if message["type"] == "error":
            print(f">>> ERR: {message.get('description','?')}", flush=True)
        return
    p = message["payload"]
    t = p.get("t")
    if t == "info":
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
    print(">>> wide-net hooks armed. Trigger render in Hwp.", flush=True)
    try:
        while True: time.sleep(0.5)
    except KeyboardInterrupt:
        session.detach()


if __name__ == "__main__":
    main()
