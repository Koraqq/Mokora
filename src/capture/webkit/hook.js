// Mokora's script in the game page of its own window on Linux (WebKitGTK).
// It runs before any of the page's own scripts (document start) and does
// three jobs.
//
// 1. Reading the game. WebKitGTK has no DevTools protocol to read a page's
//    WebSocket traffic from, so every WebSocket's life is queued here, for
//    Mokora to take (`capture::webkit` drains `window.__mokoraWsQueue`):
//
//      {k:"open",  id, url}
//      {k:"f",     id, up, op, d}   one frame: up = sent by the page; op 1 =
//                                     text (d as is), op 2 = binary (d in
//                                     base64) — the shape CDP gives frames in
//      {k:"close", id}
//
//    Messages are queued in the order things happen in the page. A frame is
//    queued in the same turn of the event loop it is sent or received in — a
//    received one from a listener added when the socket is made, so before
//    any the game adds — and only a Blob (which can be read asynchronously
//    only) waits, behind the socket's earlier frames.
//
// 2. Decompressing the game's build. Its CDN sends the `.gz` files of the
//    Unity build as `Content-Type: application/x-gzip` with
//    `Content-Encoding: gzip`; libsoup (WebKitGTK's HTTP stack) takes that
//    pair for a gzip *file* mislabelled by Apache and hands the bytes over
//    still compressed, which Chrome and Firefox do not. So a `.gz` that
//    arrives still gzip (it starts with 1f 8b) is decompressed here before
//    the page sees it: through `fetch` (the data and the WebAssembly) and
//    through a script's `src` (the framework). Anything already decoded is
//    left as it is.
//
// 3. Resting the game. Mokora keeps WebKit from treating a minimised window
//    as hidden, so a game in progress is never slowed (`hub::gtk_layout`);
//    the power saving while nothing is played is done here instead: with
//    `window.__mokoraRest` set (`hub::views::set_games_visible`), animation
//    frames come once a second, as a browser gives a hidden page.
(() => {
  if (window.__mokoraWs) return;
  Object.defineProperty(window, '__mokoraWs', { value: true });

  // ---- 3. Resting ---------------------------------------------------------

  const nativeRaf = window.requestAnimationFrame;
  if (typeof nativeRaf === 'function') {
    const REST_FRAME_MS = 1000;
    window.__mokoraRest = false;
    window.requestAnimationFrame = function (callback) {
      if (!window.__mokoraRest) return nativeRaf.call(window, callback);
      // A timer id stands in for the frame's; cancelling it still works
      // through cancelAnimationFrame below.
      return -setTimeout(() => callback(performance.now()), REST_FRAME_MS);
    };
    const nativeCancel = window.cancelAnimationFrame;
    window.cancelAnimationFrame = function (id) {
      if (id < 0) return clearTimeout(-id);
      return nativeCancel.call(window, id);
    };
  }

  // ---- 2. The build's .gz files ----------------------------------------

  const GZ = /\.gz(?:$|[?#])/;
  const isGzip = (b) => b.length >= 2 && b[0] === 0x1f && b[1] === 0x8b;
  const gunzip = (buf) =>
    new Response(new Blob([buf]).stream().pipeThrough(new DecompressionStream('gzip'))).arrayBuffer();

  const nativeFetch = window.fetch;
  if (typeof nativeFetch === 'function' && typeof DecompressionStream === 'function') {
    window.fetch = function (input, init) {
      const url = typeof input === 'string' ? input : input && input.url;
      const response = nativeFetch.apply(this, arguments);
      if (!url || !GZ.test(url)) return response;
      return response.then(async (r) => {
        if (!r.ok) return r;
        const buf = await r.arrayBuffer();
        if (!isGzip(new Uint8Array(buf, 0, Math.min(2, buf.byteLength)))) {
          return new Response(buf, { status: r.status, statusText: r.statusText, headers: r.headers });
        }
        const plain = await gunzip(buf);
        const headers = new Headers(r.headers);
        headers.delete('content-encoding');
        headers.set('content-length', String(plain.byteLength));
        return new Response(plain, { status: r.status, statusText: r.statusText, headers });
      });
    };

    const desc = Object.getOwnPropertyDescriptor(HTMLScriptElement.prototype, 'src');
    if (desc && desc.set) {
      Object.defineProperty(HTMLScriptElement.prototype, 'src', {
        configurable: true,
        enumerable: desc.enumerable,
        get: desc.get,
        set(value) {
          const url = String(value);
          if (!GZ.test(url)) return desc.set.call(this, value);
          // Load it ourselves; the script element gets the plain code (and
          // fires its own load or error) once it is ready.
          nativeFetch(url)
            .then((r) => r.arrayBuffer())
            .then((buf) => (isGzip(new Uint8Array(buf, 0, Math.min(2, buf.byteLength))) ? gunzip(buf) : buf))
            .then((code) => {
              const blob = new Blob([code], { type: 'text/javascript' });
              desc.set.call(this, URL.createObjectURL(blob));
            })
            .catch(() => desc.set.call(this, value));
        },
      });
    }
  }

  // ---- 1. The game's WebSockets -----------------------------------------

  const Native = window.WebSocket;
  if (typeof Native !== 'function') return;

  // What Mokora has not taken yet. Bounded, so a page left running while
  // nothing reads it cannot grow without end.
  const LIMIT = 20000;
  const queue = [];
  Object.defineProperty(window, '__mokoraWsQueue', { value: queue });
  const post = (m) => {
    queue.push(JSON.stringify(m));
    if (queue.length > LIMIT) queue.splice(0, queue.length - LIMIT);
  };

  const CHUNK = 0x8000;
  const base64 = (bytes) => {
    let s = '';
    for (let i = 0; i < bytes.length; i += CHUNK) {
      s += String.fromCharCode.apply(null, bytes.subarray(i, i + CHUNK));
    }
    return btoa(s);
  };

  // Socket ids are `ws<n>-<document>`: a reload starts counting again, and
  // the document part keeps its sockets apart from the last page's. The
  // number leads, so an id cut short for display still tells sockets apart.
  const doc = Math.random().toString(36).slice(2, 10);
  let next = 0;

  // Queue `data` as one frame of socket `id`, through `chain` (that socket's
  // queue for frames that have to wait).
  const frame = (id, up, data, chain) => {
    if (typeof data === 'string') {
      post({ k: 'f', id, up, op: 1, d: data });
    } else if (data instanceof ArrayBuffer) {
      post({ k: 'f', id, up, op: 2, d: base64(new Uint8Array(data)) });
    } else if (ArrayBuffer.isView(data)) {
      post({
        k: 'f',
        id,
        up,
        op: 2,
        d: base64(new Uint8Array(data.buffer, data.byteOffset, data.byteLength)),
      });
    } else if (data instanceof Blob) {
      chain.p = chain.p.then(() =>
        data.arrayBuffer().then((b) => post({ k: 'f', id, up, op: 2, d: base64(new Uint8Array(b)) })),
      );
    }
  };

  class MokoraWebSocket extends Native {
    constructor(url, protocols) {
      super(url, protocols);
      const id = 'ws' + ++next + '-' + doc;
      const chain = { p: Promise.resolve() };
      Object.defineProperty(this, '__mokora', { value: { id, chain } });
      post({ k: 'open', id, url: String(this.url) });
      this.addEventListener('message', (ev) => frame(id, false, ev.data, chain));
      this.addEventListener('close', () => {
        chain.p = chain.p.then(() => post({ k: 'close', id }));
      });
    }

    send(data) {
      const m = this.__mokora;
      // What the browser would refuse (a closed socket) never left.
      if (m && this.readyState === Native.OPEN) frame(m.id, true, data, m.chain);
      return super.send(data);
    }
  }

  window.WebSocket = MokoraWebSocket;
})();
