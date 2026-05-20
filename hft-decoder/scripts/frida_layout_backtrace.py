"""raid 24: macOS Hwp paragraph layout entry RE.

Hooks libhsp.dylib FUN_00091c30 (painter consumer, raid 22 verified) and captures
a backtrace at every invocation. The unique caller chain ladders (deduped) reveal
the paragraph layout dispatch chain on top of the painter, which is the entry
point we want to RE for line breaking / inline shape positioning.

Usage:
    1. Open Hancom Office HWP (macOS) and load the target hwpx (e.g. Q29.hwpx)
    2. frida -U -n Hwp -l frida_layout_backtrace.py
    3. In Hwp: File → Export PDF → save somewhere
    4. Wait for capture to finish, then Ctrl-D in frida console
    5. /tmp/layout_backtrace.log will contain deduped caller chains
"""
import frida
import json
import sys
from pathlib import Path

JS = r"""
'use strict';
const dyld = Process.findModuleByName('libhsp.dylib');
if (!dyld) { send({type:'err', msg:'libhsp.dylib not loaded'}); }
const painter_consumer = dyld ? dyld.base.add(0x91c30) : null;

const seen = new Set();
let count = 0;

if (painter_consumer) {
    Interceptor.attach(painter_consumer, {
        onEnter(args) {
            count += 1;
            // Capture top ~10 frames from backtrace
            const bt = Thread.backtrace(this.context, Backtracer.ACCURATE)
                .slice(0, 12)
                .map(a => {
                    const m = Process.findModuleByAddress(a);
                    if (m) {
                        return `${m.name}+0x${a.sub(m.base).toString(16)}`;
                    }
                    return a.toString();
                });
            const key = bt.join(' / ');
            if (!seen.has(key)) {
                seen.add(key);
                send({type:'bt', n:count, chain:bt});
            }
        }
    });
    send({type:'info', msg:'hook installed at libhsp.dylib+0x91c30, waiting for PDF export'});
} else {
    send({type:'err', msg:'libhsp.dylib not found - is Hwp running?'});
}

// Periodic stats
setInterval(() => {
    send({type:'stat', count:count, unique:seen.size});
}, 5000);
"""

def on_msg(msg, data):
    if msg.get('type') != 'send':
        return
    payload = msg.get('payload') or {}
    t = payload.get('type')
    if t == 'bt':
        log.write(json.dumps(payload, ensure_ascii=False) + "\n")
        log.flush()
        print(f"[#{payload['n']:5d}] unique chain (#{len(payload['chain'])} frames): {payload['chain'][0]} ← ... ← {payload['chain'][-1]}")
    elif t == 'stat':
        print(f"  total hits: {payload['count']}, unique chains: {payload['unique']}")
    elif t == 'info':
        print(f"INFO: {payload['msg']}")
    elif t == 'err':
        print(f"ERR: {payload['msg']}")

if __name__ == '__main__':
    out = Path('/tmp/layout_backtrace.log')
    log = open(out, 'w', encoding='utf-8')
    device = frida.get_usb_device(timeout=3) if '--usb' in sys.argv else frida.get_local_device()
    session = device.attach('Hwp')
    script = session.create_script(JS)
    script.on('message', on_msg)
    script.load()
    print(f"Hook loaded. Trigger PDF export in Hwp. Output → {out}. Ctrl-C to stop.")
    try:
        sys.stdin.read()
    except KeyboardInterrupt:
        pass
    print(f"Done. log: {out}")
