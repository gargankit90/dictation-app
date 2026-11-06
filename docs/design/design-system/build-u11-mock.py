#!/usr/bin/env python3
"""Builds u11-mock.html: the setup redesign (U11) in light and dark, from tokens.css."""
import os
import re

here = os.path.dirname(os.path.abspath(__file__))
tok = open(os.path.join(here, "tokens.css")).read()
root = re.search(r":root \{(.*?)\n\}", tok, re.S).group(1)
dark = re.search(r"prefers-color-scheme: dark\) \{\s*:root \{(.*?)\n  \}", tok, re.S).group(1)
vars_css = f".theme-light{{{root}}}\n.theme-dark{{{root}{dark}}}"

STEPS = ["Welcome", "Speech", "Permissions", "Try it", "Ready"]


def rail(cur):
    out = ""
    for i, s in enumerate(STEPS):
        if i < cur:
            out += f'<div class="st done"><i>✓</i>{s}</div>'
        elif i == cur:
            out += f'<div class="st cur"><i>{i + 1}</i>{s}</div>'
        else:
            out += f'<div class="st"><i>{i + 1}</i>{s}</div>'
    return f'<div class="rail">{out}</div>'


STRIP = '<div class="strip"><span>Downloading English · 170 of 477 MB</span><span class="pb"><i style="width:36%"></i></span><a>Cancel</a></div>'


def win(cur, body, foot, strip=""):
    return (
        f'<div class="win"><div class="bar"><b></b><b></b><b></b><span>Set up Vocet</span></div>'
        f'<div class="main">{rail(cur)}<div class="pane"><div class="content">{body}</div>'
        f'{strip}<div class="foot">{foot}</div></div></div></div>'
    )


WELCOME = """
<h1 class="big">Talk instead of type</h1>
<p class="lede">Vocet turns what you say into text, in any app.</p>
<div class="demo" data-demo>
  <div class="dwin">
    <div class="dmsg"><span class="av">S</span><span>Can you send the notes before Friday?</span></div>
    <div class="dbox"><span class="dtext"></span><span class="caret"></span></div>
    <div class="dpill"><span class="dot"></span><span class="spin"></span><span class="chk">✓</span><span class="meter"><i></i><i></i><i></i><i></i><i></i><i></i><i></i></span><span class="plabel">Listening</span><span class="phint">Release to paste · Esc to cancel</span></div>
  </div>
  <ol class="beats">
    <li class="b1"><kbd>Right Option (⌥)</kbd><span>Hold the key</span></li>
    <li class="b2"><em>Say what you mean</em></li>
    <li class="b3"><em>Let go. The words land at your cursor</em></li>
  </ol>
</div>
"""

SPEECH = """
<h1>Which language will you mostly speak?</h1>
<div class="chips"><span class="chip on">English</span><span class="chip">Hindi</span><span class="chip">More…</span></div>
<div class="opt"><div class="radio"></div><div><small class="rec">Recommended for your language</small><b>English · 477 MB</b><p>Runs on this Mac. Works offline after a one-time download.</p></div></div>
<div class="opt sel"><div class="radio on"></div><div><b>Use my own Groq or OpenAI key</b><p>About 100 languages. Needs internet. Your speech is sent to the provider you choose.</p></div></div>
"""

KEY_NEW = """
<h1>Connect your key</h1>
<p class="sub">Vocet sends your speech to this provider, using your own account with them.</p>
<div class="grp"><div class="r"><b>Provider</b><span class="pop">Groq ⌄</span></div></div>
<ol class="how"><li>Open <a>console.groq.com</a> and sign in.</li><li>Create an API key and copy it.</li><li>Paste it below and press Test &amp; Save.</li></ol>
<label class="lab">Groq API key</label><div class="field">Paste your key</div>
<div class="rowb"><button class="btn pri">Test &amp; Save</button></div>
"""

KEY_SAVED = """
<h1>Connect your key</h1>
<p class="sub">Vocet sends your speech to this provider, using your own account with them.</p>
<div class="grp"><div class="r"><b>Provider</b><span class="pop">Groq ⌄</span></div>
<div class="r"><div><b class="okt">✓ Key saved</b><small>In your macOS Keychain. It's never shown again.</small></div><button class="btn sec">Replace</button></div></div>
"""

SPEECH_LOCAL = """
<h1>Which language will you mostly speak?</h1>
<div class="chips"><span class="chip on">English</span><span class="chip">Hindi</span><span class="chip">More…</span></div>
<div class="opt sel"><div class="radio on"></div><div><small class="rec">Recommended for your language</small><b>English · 477 MB</b><p>Runs on this Mac. Works offline after a one-time download.</p></div></div>
<div class="opt"><div class="radio"></div><div><b>Use my own Groq or OpenAI key</b><p>About 100 languages. Needs internet. Your speech is sent to the provider you choose.</p></div></div>
"""

SPEECH_HINDI = """
<h1>Which language will you mostly speak?</h1>
<div class="chips"><span class="chip">English</span><span class="chip on">Hindi</span><span class="chip">More…</span></div>
<div class="opt sel"><div class="radio on"></div><div><small class="rec">Recommended for your language</small><b>30 languages · 590 MB</b><p>Runs on this Mac. Works offline after a one-time download.</p></div></div>
<div class="opt"><div class="radio"></div><div><b>Use my own Groq or OpenAI key</b><p>About 100 languages. Needs internet. Your speech is sent to the provider you choose.</p></div></div>
"""

SPEECH_UNCOVERED = """
<h1>Which language will you mostly speak?</h1>
<div class="chips"><span class="chip">English</span><span class="chip">Hindi</span><span class="chip on">Gujarati</span></div>
<p class="sub">Gujarati needs your own key. Vocet's on-device models don't cover it yet.</p>
<div class="opt sel"><div class="radio on"></div><div><b>Use my own Groq or OpenAI key</b><p>About 100 languages. Needs internet. Your speech is sent to the provider you choose.</p></div></div>
"""

PERMS = """
<h1>Two permissions</h1>
<p class="sub">macOS asks you to confirm each one. This screen updates on its own.</p>
<div class="opt"><div style="flex:1"><b>Microphone</b><p>Records only while you hold your key.</p></div><b class="okt">✓ Allowed</b></div>
<div class="opt"><div style="flex:1"><b>Accessibility</b><p>Notices your key and pastes at your cursor. It doesn't read your screen.</p></div><b class="okt">✓ Allowed</b></div>
"""

TRY_WAIT = """
<h1>Try it</h1>
<div class="keyrow"><span class="lab2">Your dictation key</span><kbd>Right ⌥</kbd><button class="link">Change key</button></div>
<p class="ins">Hold the key, say a sentence, let go.</p>
<div class="area wait">Getting the model ready: 37%</div>
<div class="stages"><span>· Heard you</span><span>· Transcribed</span><span>· Pasted</span></div>
"""

TRY_FAIL = """
<h1>Try it</h1>
<div class="keyrow"><span class="lab2">Your dictation key</span><kbd>Right ⌥ + Right ⌘</kbd><button class="link">Change key</button></div>
<p class="ins">Hold the key, say a sentence, let go.</p>
<div class="area small"></div>
<div class="stages"><span class="ok">✓ Heard you</span><span class="ok">✓ Transcribed</span><span>· Pasted</span></div>
<div class="fail"><p>Vocet heard you, but the words didn't reach the box:</p><div class="words">Good morning, how are you doing?</div><div class="failrow"><button class="btn sec">Copy</button><small>Click the box and press ⌘V. Vocet never pastes again by itself, so nothing lands twice.</small></div></div>
"""

TRY = """
<h1>Try it</h1>
<div class="keyrow"><span class="lab2">Your dictation key</span><kbd>Right ⌥</kbd><button class="link">Change key</button></div>
<p class="ins">Hold the key, say a sentence, let go.</p>
<div class="area"><span class="caret on"></span></div>
<div class="stages"><span>· Heard you</span><span>· Transcribed</span><span>· Pasted</span></div>
"""


def frames(theme):
    b_back = '<button class="btn sec lg">Back</button>'
    fs = [
        ("Welcome: shows what Vocet does, asks for nothing", win(0, WELCOME, '<span class="sp"></span><button class="btn pri lg">Get started</button>')),
        ("Speech, the usual route: on-device model recommended", win(1, SPEECH_LOCAL, b_back + '<span class="sp"></span><button class="btn pri lg">Download and continue</button>')),
        ("Speech, Hindi: 30 languages", win(1, SPEECH_HINDI, b_back + '<span class="sp"></span><button class="btn pri lg">Download and continue</button>')),
        ("Speech, a language no on-device model covers", win(1, SPEECH_UNCOVERED, b_back + '<span class="sp"></span><button class="btn pri lg">Continue</button>')),
        ("Permissions: unchanged, with the download strip", win(2, PERMS, b_back + '<span class="sp"></span><button class="btn pri lg">Continue</button>', STRIP)),
        ("Speech, own key chosen: no form here, Continue opens the next screen", win(1, SPEECH, b_back + '<span class="sp"></span><button class="btn pri lg">Continue</button>')),
        ("Own key: its own screen, with Back", win(1, KEY_NEW, b_back + '<span class="sp"></span><span class="why">Continue unlocks after Test &amp; Save succeeds</span><button class="btn pri lg dis">Continue</button>')),
        ("Own key, already saved: Key saved and Replace, no Remove key", win(1, KEY_SAVED, b_back + '<span class="sp"></span><button class="btn pri lg">Continue</button>')),
        ("Try it, model still downloading: practice waits, strip stays", win(3, TRY_WAIT, b_back + '<span class="sp"></span><span class="why">Continue unlocks after a dictation lands in the box</span><button class="btn sec lg">Skip for now</button><button class="btn pri lg dis">Continue</button>', STRIP)),
        ("Try it, failed paste with a two-key shortcut: the whole panel is in view", win(3, TRY_FAIL, b_back + '<span class="sp"></span><span class="why">Continue unlocks after a dictation lands in the box</span><button class="btn sec lg">Skip for now</button><button class="btn pri lg dis">Continue</button>')),
        ("Try it: small Change key, no Reset to default, shorter instruction", win(3, TRY, b_back + '<span class="sp"></span><span class="why">Continue unlocks after a dictation lands in the box</span><button class="btn sec lg">Skip for now</button><button class="btn pri lg dis">Continue</button>')),
    ]
    out = "".join(f"<figure><figcaption>{c}</figcaption>{w}</figure>" for c, w in fs)
    return f'<section class="{theme} col"><h2>{"Light" if theme == "theme-light" else "Dark"}</h2>{out}</section>'


CSS = """
body{margin:0;font-family:-apple-system,BlinkMacSystemFont,system-ui,sans-serif;background:var(--background,#fff);color:var(--foreground,#111)}
.top{padding:14px 18px 0}.top h1{margin:0;font-size:19px}.top p{margin:4px 0 0;font-size:13px;opacity:.8}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(600px,1fr));gap:14px;padding:14px}
.col{background:var(--vc-bg-grouped);border-radius:14px;padding:14px;color:var(--vc-ink)}
.col h2{margin:0 0 8px;font-size:12px;letter-spacing:.06em;text-transform:uppercase;color:var(--vc-muted)}
figure{margin:0 0 16px}figcaption{font-size:12px;margin-bottom:6px;color:var(--vc-ink-2)}
.win{width:100%;max-width:740px;border:1px solid var(--vc-line-strong);border-radius:12px;overflow:hidden;background:var(--vc-bg);font:13px/1.45 var(--vc-font-ui);box-shadow:var(--vc-shadow-card)}
.bar{display:flex;gap:6px;align-items:center;height:26px;padding:0 10px;background:var(--vc-bg-grouped);border-bottom:1px solid var(--vc-line);position:relative}
.bar b{width:10px;height:10px;border-radius:50%;background:var(--vc-line-strong)}.bar span{position:absolute;left:0;right:0;text-align:center;font-weight:600;font-size:12px;color:var(--vc-muted)}
.main{display:grid;grid-template-columns:136px 1fr;height:420px}
.rail{background:var(--vc-bg-grouped);border-right:1px solid var(--vc-line);padding:12px 8px}
.st{display:flex;align-items:center;gap:8px;padding:6px 8px;border-radius:8px;color:var(--vc-muted)}
.st i{width:18px;height:18px;border-radius:50%;border:1px solid var(--vc-line-strong);display:inline-flex;align-items:center;justify-content:center;font-style:normal;font-size:11px}
.st.cur{background:var(--vc-accent-soft);color:var(--vc-ink);font-weight:600}.st.cur i{background:var(--vc-primary-bg);border-color:var(--vc-primary-bg);color:#fff}
.st.done{color:var(--vc-ink-2)}.st.done i{border-color:var(--vc-ready);color:var(--vc-ready)}
.pane{display:flex;flex-direction:column;min-width:0}.content{flex:1;padding:20px 22px;overflow:hidden}
.strip{display:flex;align-items:center;gap:10px;padding:7px 14px;border-top:1px solid var(--vc-line);background:var(--vc-bg-grouped);font-size:12px}.strip .pb{flex:1;height:4px;border-radius:3px;background:var(--vc-line-strong);overflow:hidden}.strip .pb i{display:block;height:100%;background:var(--vc-primary-bg)}.strip a{color:var(--vc-accent);text-decoration:underline}
.foot{display:flex;align-items:center;gap:10px;padding:10px 14px;border-top:1px solid var(--vc-line)}.sp{flex:1}
.why{font-size:11px;color:var(--vc-muted);max-width:170px;text-align:right}
h1{margin:0 0 10px;font-size:var(--vc-text-2xl);font-weight:650}h1.big{font-size:var(--vc-text-3xl);letter-spacing:-.02em;margin-bottom:4px}
.lede{margin:0 0 14px;font-size:var(--vc-text-lg);color:var(--vc-ink-2)}.sub{margin:-4px 0 12px;color:var(--vc-muted)}
.btn{font:inherit;font-weight:500;height:var(--vc-control-h);padding:0 12px;border-radius:var(--vc-radius-sm);border:1px solid transparent;white-space:nowrap}
.btn.lg{height:36px;padding:0 16px;font-size:14px;border-radius:9px;font-weight:600}
.pri{background:var(--vc-primary-bg);color:var(--vc-primary-fg)}.pri.dis{opacity:.5}.sec{background:var(--vc-bg);color:var(--vc-ink);border-color:var(--vc-line-strong)}
.link{background:none;border:0;color:var(--vc-accent);font:inherit;padding:0 2px;cursor:pointer}
kbd{font:600 12px var(--vc-font-ui);background:var(--vc-bg-grouped);border:1px solid var(--vc-line-strong);border-bottom-width:3px;border-radius:6px;padding:3px 9px;color:var(--vc-ink);white-space:nowrap;display:inline-block;line-height:1.35}
.demo{border:1px solid var(--vc-line);border-radius:var(--vc-radius-lg);padding:14px;background:var(--vc-bg-grouped)}
.dwin{position:relative;background:var(--vc-bg);border:1px solid var(--vc-line);border-radius:10px;padding:12px;height:112px}
.dmsg{display:flex;gap:8px;align-items:center;font-size:12px;color:var(--vc-ink-2)}.av{width:22px;height:22px;border-radius:6px;background:var(--vc-bg-grouped);display:inline-flex;align-items:center;justify-content:center;font-weight:700;font-size:11px;flex:none}
.dbox{margin-top:10px;border:1.5px solid var(--vc-accent);border-radius:8px;padding:7px 9px;min-height:32px;box-sizing:border-box;font-size:13px}
.caret{display:inline-block;width:1.5px;height:14px;background:var(--vc-accent);vertical-align:-2px;animation:blink 1.1s steps(1) infinite}@keyframes blink{50%{opacity:0}}
.dpill{position:absolute;left:50%;bottom:-20px;transform:translateX(-50%);display:flex;align-items:center;gap:7px;height:40px;padding:0 16px;border-radius:999px;background:var(--vc-overlay-bg);color:var(--vc-overlay-fg);font-size:15px;font-weight:600;opacity:0;transition:opacity .2s;white-space:nowrap}
.spin{display:none;width:10px;height:10px;border:2px solid var(--vc-meter-idle);border-top-color:var(--vc-meter);border-radius:50%;animation:sp .8s linear infinite}@keyframes sp{to{transform:rotate(360deg)}}.chk{display:none;color:var(--vc-overlay-ready);font-weight:800}
.dpill[data-s=t] .spin,.dpill[data-s=d] .chk{display:inline-block}.dpill[data-s=t] .dot,.dpill[data-s=t] .meter,.dpill[data-s=d] .dot,.dpill[data-s=d] .meter{display:none}
.phint{font-size:12px;font-weight:500;color:var(--vc-overlay-muted)}.dpill[data-s=t] .phint,.dpill[data-s=d] .phint{display:none}
.dot{width:8px;height:8px;border-radius:50%;background:var(--vc-recording)}.meter{display:inline-flex;gap:2px;align-items:center;height:18px}.meter i{width:3px;height:4px;transition:height 50ms linear;border-radius:2px;background:var(--vc-meter)}
.beats{list-style:none;margin:30px 0 0;padding:0;display:grid;grid-template-columns:repeat(3,1fr);gap:8px}
.beats li{display:flex;flex-direction:column;gap:6px;align-items:flex-start;font-size:12px;color:var(--vc-muted);transition:color .2s}.beats em{font-style:normal}.beats li.on{color:var(--vc-ink);font-weight:600}
.chips{display:flex;gap:8px;margin-bottom:12px}.chip{border:1px solid var(--vc-line-strong);border-radius:7px;padding:4px 12px}.chip.on{background:var(--vc-primary-bg);border-color:var(--vc-primary-bg);color:#fff;font-weight:600}
.opt{display:flex;gap:10px;border:1px solid var(--vc-line);border-radius:10px;padding:10px 12px;margin-bottom:8px}.opt.sel{border:1.5px solid var(--vc-accent)}.opt p{margin:2px 0 0;font-size:12px;color:var(--vc-muted)}.opt b{display:block}.rec{color:var(--vc-accent);font-weight:600}
.radio{width:13px;height:13px;border-radius:50%;border:1px solid var(--vc-line-strong);margin-top:3px;flex:none}.radio.on{border:4px solid var(--vc-accent);width:7px;height:7px}
.grp{background:var(--vc-bg-grouped);border-radius:var(--vc-radius-md);margin-bottom:12px}.r{display:flex;justify-content:space-between;align-items:center;gap:10px;padding:9px 12px;border-top:1px solid var(--vc-line)}.r:first-child{border-top:0}.r small{display:block;color:var(--vc-muted);font-size:12px}
.pop{border:1px solid var(--vc-line-strong);border-radius:6px;padding:2px 8px;background:var(--vc-bg);font-size:12px}.okt{color:var(--vc-ready)}
.how{margin:0 0 12px;padding-left:18px}.how a{color:var(--vc-accent);text-decoration:underline}.lab{display:block;font-weight:600;margin-bottom:4px}
.field{border:1px solid var(--vc-line-strong);border-radius:6px;padding:6px 9px;color:var(--vc-faint);background:var(--vc-bg);margin-bottom:10px}
.keyrow{display:flex;align-items:center;gap:10px;margin-bottom:14px}.lab2{color:var(--vc-muted)}.ins{margin:0 0 8px;font-weight:600}
.area{border:1.5px solid var(--vc-accent);border-radius:9px;height:84px;padding:9px 11px;background:var(--vc-bg);box-sizing:border-box}.caret.on{display:inline-block}
.area.wait{color:var(--vc-faint)}.area.small{height:44px}.stages span.ok{background:var(--vc-ready-soft);color:var(--vc-ready)}
.fail{margin-top:10px;background:var(--vc-attention-soft);border-radius:10px;padding:9px 12px}.fail p{margin:0 0 6px;font-size:12px}.words{background:var(--vc-bg);border:1px solid var(--vc-line);border-radius:6px;padding:5px 9px;margin-bottom:7px}.failrow{display:flex;gap:10px;align-items:center}.failrow small{font-size:11px;color:var(--vc-ink-2)}
.stages{display:flex;gap:8px;margin-top:10px}.stages span{background:var(--vc-bg-grouped);border-radius:999px;padding:3px 10px;font-size:12px;font-weight:600;color:var(--vc-muted)}
@media (prefers-reduced-motion:reduce){.caret{animation:none}}
"""

JS = """
const SPEECH=[4,6,12,16,18,14,9,5,4,8,14,17,12,7,4,4,10,16,18,16,11,6,4,4,4,7,13,17,15,10,5,4];
const TEXT='Yes, I\\u2019ll send them tomorrow morning.';
const reduce=matchMedia('(prefers-reduced-motion:reduce)').matches;
document.querySelectorAll('[data-demo]').forEach(d=>{
  const t=d.querySelector('.dtext'),p=d.querySelector('.dpill'),b=[...d.querySelectorAll('.beats li')],m=[...d.querySelectorAll('.meter i')];
  const on=i=>b.forEach((x,k)=>x.classList.toggle('on',k===i));
  if(reduce){t.textContent=TEXT;on(2);return;}
  const l=d.querySelector('.plabel');
  const state=(k,txt)=>{p.dataset.s=k;l.textContent=txt;};
  let level;
  const run=()=>{t.textContent='';on(0);state('l','Listening');p.style.opacity=1;m.forEach(x=>x.style.height='5px');
    setTimeout(()=>{on(1);let i=0;const H=[];level=setInterval(()=>{H.push(SPEECH[i++%SPEECH.length]);if(H.length>7)H.shift();m.forEach((x,k)=>{const v=H[H.length-7+k];x.style.height=(v===undefined?4:v)+'px';});},60);},900);
    setTimeout(()=>{clearInterval(level);on(2);state('t','Transcribing\u2026');},3000);
    setTimeout(()=>{t.textContent=TEXT;state('d','Done');},4700);
    setTimeout(()=>{p.style.opacity=0;},5500);
    setTimeout(run,8000);};
  run();
});
"""

page = (
    f"<!doctype html><html><head><meta charset='utf-8'><title>Vocet setup, U11 mock</title><style>{vars_css}\n{CSS}</style></head><body>"
    "<div class='top'><h1>Vocet setup redesign (U11) · mock</h1><p>Welcome is animated. Permissions and Ready don't change here.</p></div>"
    f"<div class='grid'>{frames('theme-light')}{frames('theme-dark')}</div><script>{JS}</script></body></html>"
)
open(os.path.join(here, "u11-mock.html"), "w").write(page)
sd = os.environ.get("ORCA_CHAT_SCRATCH_DIR")
if sd:
    open(os.path.join(sd, "vocet-u11-mock-b7d1.html"), "w").write(page)
print(len(page))
