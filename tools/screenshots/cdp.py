#!/usr/bin/env python3
"""Tiny Chrome DevTools Protocol client for the screenshot generator.

Used to dismiss cookie-consent dialogs deterministically (click the accept
button by its text, in the page and any same-origin iframes) instead of guessing
a pixel, and to wait for a page to load instead of guessing how long it takes.
Chromium must be launched with --remote-debugging-port=PORT.

    cdp.py PORT accept         # click an "Accept/Agree/Tout accepter" button
    cdp.py PORT loaded [SECS]  # wait until the page has loaded (default 60 s)

accept exits 0 on success (something clicked) or if there was nothing to do;
loaded exits 1 when the page is still loading at the deadline.
"""
import asyncio
import json
import sys
import urllib.request

import websockets

# The window opens on about:blank and stays there until the server answers, which
# some sites take tens of seconds to do.
LOADED_JS = "location.href !== 'about:blank' && document.readyState === 'complete'"

ACCEPT_JS = r"""
(() => {
  const re = /^(accept|agree|i agree|accept all|allow all|got it|ok|tout accepter|j'accepte|accepter)/i;
  const pick = (doc) => {
    const els = [...doc.querySelectorAll('button,a,[role="button"],input[type="button"],input[type="submit"]')];
    return els.find(e => re.test(((e.textContent || e.value || '')).trim()));
  };
  let btn = pick(document);
  if (!btn) {
    for (const f of document.querySelectorAll('iframe')) {
      try { const b = pick(f.contentDocument); if (b) { btn = b; break; } } catch (e) {}
    }
  }
  if (btn) { btn.click(); return 'clicked: ' + (btn.textContent || btn.value || '').trim().slice(0, 40); }
  return 'nothing-to-click';
})()
"""


def page_target(port: int) -> dict | None:
    targets = json.load(urllib.request.urlopen(f"http://127.0.0.1:{port}/json", timeout=5))
    return next((t for t in targets if t.get("type") == "page" and "webSocketDebuggerUrl" in t), None)


async def evaluate(ws_url: str, expr: str):
    """The value of EXPR in the top document."""
    async with websockets.connect(ws_url, max_size=None) as ws:
        await ws.send(json.dumps({"id": 1, "method": "Runtime.evaluate",
                                  "params": {"expression": expr, "returnByValue": True}}))
        while True:
            msg = json.loads(await asyncio.wait_for(ws.recv(), timeout=5))
            if msg.get("id") == 1:
                return msg.get("result", {}).get("result", {}).get("value")


async def wait_loaded(port: int, secs: float) -> bool:
    loop = asyncio.get_running_loop()
    deadline = loop.time() + secs
    while loop.time() < deadline:
        try:
            page = page_target(port)
            if page and await evaluate(page["webSocketDebuggerUrl"], LOADED_JS) is True:
                return True
        except (OSError, asyncio.TimeoutError, websockets.WebSocketException):
            pass
        await asyncio.sleep(0.5)
    return False


async def run(port: int, action: str) -> str:
    page = page_target(port)
    if not page:
        return "no-page-target"
    expr = ACCEPT_JS if action == "accept" else action

    async with websockets.connect(page["webSocketDebuggerUrl"], max_size=None) as ws:
        nid = 0

        async def send(method, params=None, session=None):
            nonlocal nid
            nid += 1
            m = {"id": nid, "method": method, "params": params or {}}
            if session:
                m["sessionId"] = session
            await ws.send(json.dumps(m))
            return nid

        # Auto-attach to child frames (cross-origin consent CMPs live in OOPIFs,
        # which the top document's JS can't reach). flatten => sessionId routing.
        await send("Target.setAutoAttach",
                   {"autoAttach": True, "waitForDebuggerOnStart": False, "flatten": True})

        # Collect the attached iframe sessions for a short window.
        sessions = [None]  # None = the top page (default session)
        try:
            while True:
                msg = json.loads(await asyncio.wait_for(ws.recv(), timeout=1.5))
                if msg.get("method") == "Target.attachedToTarget":
                    sessions.append(msg["params"]["sessionId"])
        except asyncio.TimeoutError:
            pass

        # Run the accept JS in the top page and in every attached frame; first hit wins.
        results = []
        for sess in sessions:
            mid = await send("Runtime.evaluate",
                             {"expression": expr, "returnByValue": True, "awaitPromise": True},
                             session=sess)
            try:
                while True:
                    msg = json.loads(await asyncio.wait_for(ws.recv(), timeout=2.0))
                    if msg.get("id") == mid:
                        val = msg.get("result", {}).get("result", {}).get("value")
                        results.append(val)
                        if isinstance(val, str) and val.startswith("clicked"):
                            return val
                        break
            except asyncio.TimeoutError:
                continue
        return "; ".join(str(r) for r in results if r) or "nothing-to-click"


def main() -> int:
    if len(sys.argv) < 3:
        print("usage: cdp.py PORT accept|loaded [SECS]|<js>", file=sys.stderr)
        return 2
    if sys.argv[2] == "loaded":
        secs = float(sys.argv[3]) if len(sys.argv) > 3 else 60.0
        loaded = asyncio.run(wait_loaded(int(sys.argv[1]), secs))
        print("loaded" if loaded else f"still loading after {secs:g} s")
        return 0 if loaded else 1
    try:
        print(asyncio.run(run(int(sys.argv[1]), sys.argv[2])))
        return 0
    except Exception as e:  # best-effort: never break a scene over consent
        print(f"cdp error: {e}", file=sys.stderr)
        return 0


if __name__ == "__main__":
    sys.exit(main())
