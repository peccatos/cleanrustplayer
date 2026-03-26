pub fn youtube_player_page(initial_query: Option<&str>, initial_url: Option<&str>) -> String {
    let initial_query =
        serde_json::to_string(initial_query.unwrap_or("")).unwrap_or_else(|_| "\"\"".to_string());
    let initial_url =
        serde_json::to_string(initial_url.unwrap_or("")).unwrap_or_else(|_| "\"\"".to_string());

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>ReplayCore YouTube</title>
  <style>
    :root {{
      color-scheme: dark;
      --bg: #0b0d12;
      --panel: rgba(15, 19, 28, 0.86);
      --panel-strong: rgba(20, 25, 37, 0.96);
      --line: rgba(255, 255, 255, 0.08);
      --text: #f2f4f8;
      --muted: #aab2c0;
      --accent: #8bc1ff;
      --accent-2: #9fffd1;
      --danger: #ff7b7b;
    }}
    * {{ box-sizing: border-box; }}
    html, body {{
      min-height: 100%;
      margin: 0;
      background:
        radial-gradient(circle at top left, rgba(121, 167, 255, 0.18), transparent 32%),
        radial-gradient(circle at top right, rgba(159, 255, 209, 0.12), transparent 26%),
        linear-gradient(180deg, #07090d 0%, #0b0d12 100%);
      color: var(--text);
      font-family: "Segoe UI", "SF Pro Display", "Helvetica Neue", sans-serif;
    }}
    body {{
      padding: 24px;
    }}
    .shell {{
      max-width: 1300px;
      margin: 0 auto;
    }}
    .hero {{
      display: flex;
      justify-content: space-between;
      align-items: flex-end;
      gap: 24px;
      margin-bottom: 20px;
    }}
    .brand {{
      display: grid;
      gap: 8px;
    }}
    .eyebrow {{
      color: var(--accent);
      text-transform: uppercase;
      letter-spacing: 0.18em;
      font-size: 12px;
      font-weight: 700;
    }}
    h1 {{
      margin: 0;
      font-size: clamp(34px, 5vw, 64px);
      line-height: 0.95;
      letter-spacing: -0.04em;
    }}
    .subtitle {{
      color: var(--muted);
      max-width: 62ch;
      line-height: 1.45;
      font-size: 15px;
    }}
    .meta {{
      text-align: right;
      color: var(--muted);
      font-size: 13px;
      line-height: 1.5;
    }}
    .grid {{
      display: grid;
      grid-template-columns: 360px minmax(0, 1fr) 380px;
      gap: 18px;
      align-items: start;
    }}
    .panel {{
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 20px;
      box-shadow: 0 20px 60px rgba(0, 0, 0, 0.28);
      backdrop-filter: blur(12px);
      overflow: hidden;
    }}
    .panel .inner {{
      padding: 18px;
    }}
    .panel h2 {{
      margin: 0 0 14px;
      font-size: 15px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--muted);
    }}
    label {{
      display: block;
      font-size: 12px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--muted);
      margin-bottom: 6px;
    }}
    input, button {{
      font: inherit;
    }}
    input[type="text"] {{
      width: 100%;
      border: 1px solid var(--line);
      background: var(--panel-strong);
      color: var(--text);
      border-radius: 14px;
      padding: 14px 15px;
      outline: none;
      transition: border-color 0.15s ease, transform 0.15s ease, box-shadow 0.15s ease;
    }}
    input[type="text"]:focus {{
      border-color: rgba(139, 193, 255, 0.72);
      box-shadow: 0 0 0 4px rgba(139, 193, 255, 0.12);
    }}
    .row {{
      display: grid;
      gap: 12px;
      margin-bottom: 14px;
    }}
    .actions {{
      display: flex;
      flex-wrap: wrap;
      gap: 10px;
      margin-top: 12px;
    }}
    button {{
      border: 0;
      border-radius: 999px;
      padding: 11px 16px;
      cursor: pointer;
      background: linear-gradient(135deg, #8bc1ff 0%, #9fffd1 100%);
      color: #081018;
      font-weight: 700;
      box-shadow: 0 10px 24px rgba(139, 193, 255, 0.18);
    }}
    button.secondary {{
      background: rgba(255, 255, 255, 0.08);
      color: var(--text);
      box-shadow: none;
      border: 1px solid var(--line);
    }}
    button.danger {{
      background: rgba(255, 123, 123, 0.12);
      color: #ffd0d0;
      border: 1px solid rgba(255, 123, 123, 0.24);
    }}
    button:disabled {{
      opacity: 0.55;
      cursor: not-allowed;
    }}
    .status {{
      margin-top: 12px;
      color: var(--muted);
      font-size: 14px;
      line-height: 1.5;
      min-height: 48px;
      white-space: pre-wrap;
    }}
    .player-wrap {{
      display: grid;
      gap: 12px;
    }}
    .player-stage {{
      position: relative;
      aspect-ratio: 16 / 9;
      border-radius: 18px;
      overflow: hidden;
      background: #000;
      border: 1px solid var(--line);
    }}
    #player {{
      width: 100%;
      height: 100%;
    }}
    .now-playing {{
      padding: 16px 18px;
      background: rgba(255, 255, 255, 0.04);
      border: 1px solid var(--line);
      border-radius: 16px;
    }}
    .now-playing .title {{
      font-size: 18px;
      font-weight: 700;
      margin-bottom: 4px;
    }}
    .now-playing .artist {{
      color: var(--muted);
      margin-bottom: 10px;
    }}
    .now-playing .url {{
      font-size: 12px;
      color: var(--accent);
      word-break: break-all;
    }}
    .results {{
      display: grid;
      gap: 10px;
      max-height: calc(100vh - 220px);
      overflow: auto;
      padding-right: 4px;
    }}
    .result {{
      display: grid;
      gap: 8px;
      border: 1px solid var(--line);
      background: rgba(255, 255, 255, 0.03);
      border-radius: 16px;
      padding: 14px;
    }}
    .result .name {{
      font-size: 15px;
      font-weight: 700;
      line-height: 1.4;
    }}
    .result .artist {{
      color: var(--muted);
      font-size: 13px;
    }}
    .result .url {{
      color: var(--accent);
      font-size: 12px;
      word-break: break-all;
    }}
    .result .controls {{
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
    }}
    .empty {{
      color: var(--muted);
      border: 1px dashed var(--line);
      border-radius: 16px;
      padding: 18px;
      text-align: center;
    }}
    @media (max-width: 1180px) {{
      .grid {{
        grid-template-columns: 1fr;
      }}
      .results {{
        max-height: none;
      }}
      .hero {{
        flex-direction: column;
        align-items: flex-start;
      }}
      .meta {{
        text-align: left;
      }}
    }}
  </style>
</head>
<body>
  <div class="shell">
    <header class="hero">
      <div class="brand">
        <div class="eyebrow">ReplayCore / YouTube web playback</div>
        <h1>Search, resolve, play.</h1>
        <div class="subtitle">
          Local page powered by the ReplayCore backend and the YouTube IFrame Player API.
          Search results come from the YouTube Data API; playback happens in the browser.
        </div>
      </div>
      <div class="meta">
        <div>Endpoint: <code>/web/youtube</code></div>
        <div>Search API: <code>/v1/youtube/search</code></div>
        <div>Resolve API: <code>/v1/youtube/resolve</code></div>
      </div>
    </header>

    <section class="grid">
      <div class="panel">
        <div class="inner">
          <h2>Search</h2>
          <div class="row">
            <div>
              <label for="queryInput">Query</label>
              <input id="queryInput" type="text" placeholder="daft punk" />
            </div>
            <div>
              <label for="urlInput">YouTube URL</label>
              <input id="urlInput" type="text" placeholder="https://www.youtube.com/watch?v=..." />
            </div>
          </div>
          <div class="actions">
            <button id="searchButton" onclick="runSearch()">Search</button>
            <button class="secondary" onclick="playFromUrl()">Play URL</button>
            <button class="secondary" onclick="clearResults()">Clear results</button>
          </div>
          <div id="status" class="status">Ready.</div>
        </div>
      </div>

      <div class="panel">
        <div class="inner player-wrap">
          <h2>Player</h2>
          <div class="player-stage">
            <div id="player"></div>
          </div>
          <div class="actions">
            <button class="secondary" onclick="resumePlayback()">Play</button>
            <button class="secondary" onclick="pausePlayback()">Pause</button>
            <button class="danger" onclick="stopPlayback()">Stop</button>
          </div>
          <div id="nowPlaying" class="now-playing">
            <div class="title">Nothing loaded</div>
            <div class="artist">Use search or paste a URL.</div>
            <div class="url">The player will load a YouTube video id here.</div>
          </div>
        </div>
      </div>

      <div class="panel">
        <div class="inner">
          <h2>Results</h2>
          <div id="results" class="results">
            <div class="empty">Search results will appear here.</div>
          </div>
        </div>
      </div>
    </section>
  </div>

  <script>
    const INITIAL_QUERY = {initial_query};
    const INITIAL_URL = {initial_url};
    const SEARCH_API = "/v1/youtube/search";
    const RESOLVE_API = "/v1/youtube/resolve";

    let player = null;
    let pendingVideoId = null;
    let currentVideoId = null;

    const queryInput = document.getElementById("queryInput");
    const urlInput = document.getElementById("urlInput");
    const statusEl = document.getElementById("status");
    const resultsEl = document.getElementById("results");
    const nowPlayingEl = document.getElementById("nowPlaying");

    function setStatus(message, kind = "info") {{
      statusEl.textContent = message;
      statusEl.style.color = kind === "error" ? "var(--danger)" : "var(--muted)";
    }}

    function setNowPlaying(title, artist, url) {{
      nowPlayingEl.innerHTML = `
        <div class="title">${{escapeHtml(title || "Unknown title")}}</div>
        <div class="artist">${{escapeHtml(artist || "Unknown artist")}}</div>
        <div class="url">${{escapeHtml(url || "")}}</div>
      `;
    }}

    function clearResults() {{
      resultsEl.innerHTML = '<div class="empty">Search results will appear here.</div>';
      setStatus("Results cleared.");
    }}

    function escapeHtml(value) {{
      return String(value)
        .replaceAll("&", "&amp;")
        .replaceAll("<", "&lt;")
        .replaceAll(">", "&gt;")
        .replaceAll('"', "&quot;")
        .replaceAll("'", "&#039;");
    }}

    function extractVideoId(input) {{
      if (!input) return null;
      try {{
        const url = new URL(input, window.location.origin);
        const host = url.hostname.replace(/^www\./, "").replace(/^m\./, "");
        if (host === "youtu.be") {{
          const parts = url.pathname.split("/").filter(Boolean);
          return parts[0] || null;
        }}
        if (host === "youtube.com" || host.endsWith(".youtube.com") || host === "youtube-nocookie.com") {{
          if (url.pathname === "/watch") {{
            return url.searchParams.get("v");
          }}
          const parts = url.pathname.split("/").filter(Boolean);
          if (["embed", "shorts", "live", "v"].includes(parts[0])) {{
            return parts[1] || null;
          }}
        }}
      }} catch (err) {{
        return null;
      }}
      return null;
    }}

    async function fetchEnvelope(url) {{
      const response = await fetch(url, {{
        headers: {{
          "Accept": "application/json",
        }},
      }});
      const payload = await response.json().catch(() => ({{ ok: false, error: {{ message: "invalid JSON response" }} }}));
      if (!response.ok || payload.ok === false) {{
        const message = payload?.error?.message || payload?.message || response.statusText;
        throw new Error(message);
      }}
      return payload.data;
    }}

    async function runSearch() {{
      const query = queryInput.value.trim();
      if (!query) {{
        setStatus("Enter a search query.");
        return;
      }}

      setStatus(`Searching YouTube for "${{query}}"...`);
      resultsEl.innerHTML = '<div class="empty">Searching...</div>';

      try {{
        const items = await fetchEnvelope(`${{SEARCH_API}}?q=${{encodeURIComponent(query)}}`);
        renderResults(items || []);
        setStatus(items.length ? `Found ${{items.length}} result(s).` : "No results.");
      }} catch (err) {{
        resultsEl.innerHTML = `<div class="empty">${{escapeHtml(err.message || String(err))}}</div>`;
        setStatus(err.message || String(err), "error");
      }}
    }}

    async function playFromUrl() {{
      const url = urlInput.value.trim();
      if (!url) {{
        setStatus("Paste a YouTube URL first.");
        return;
      }}

      setStatus("Resolving YouTube URL...");
      try {{
        const resolved = await fetchEnvelope(`${{RESOLVE_API}}?url=${{encodeURIComponent(url)}}`);
        const videoId = extractVideoId(resolved.page_url) || extractVideoId(resolved.preview_url) || extractVideoId(url);
        if (!videoId) {{
          throw new Error("Could not extract a video id from the resolved URL.");
        }}
        loadVideo(videoId, resolved.title, resolved.artist, resolved.page_url);
        setStatus(`Loaded "${{resolved.title}}"`);
      }} catch (err) {{
        setStatus(err.message || String(err), "error");
      }}
    }}

    function renderResults(items) {{
      if (!items.length) {{
        resultsEl.innerHTML = '<div class="empty">No results.</div>';
        return;
      }}

      resultsEl.innerHTML = items.map((item, index) => {{
        const videoId = extractVideoId(item.url);
        const title = item.title || "Unknown title";
        const artist = item.artist || "Unknown artist";
        const previewUrl = item.preview_url || "";
        return `
          <article class="result">
            <div class="name">${{escapeHtml(title)}}</div>
            <div class="artist">${{escapeHtml(artist)}}</div>
            <div class="url">${{escapeHtml(item.url)}}</div>
            <div class="controls">
              <button onclick="loadFromResult(${{JSON.stringify(videoId)}},${{JSON.stringify(title)}},${{JSON.stringify(artist)}},${{JSON.stringify(item.url)}})">Play</button>
              <button class="secondary" onclick="copyText(${{JSON.stringify(item.url)}})">Copy URL</button>
              ${{previewUrl ? `<button class="secondary" onclick="openPreview(${{JSON.stringify(previewUrl)}})">Preview</button>` : ""}}
            </div>
          </article>
        `;
      }}).join("");
    }}

    function loadFromResult(videoId, title, artist, url) {{
      if (!videoId) {{
        setStatus("Could not extract a YouTube video id.", "error");
        return;
      }}
      loadVideo(videoId, title, artist, url);
      setStatus(`Loaded "${{title}}"`);
    }}

    function loadVideo(videoId, title, artist, url) {{
      currentVideoId = videoId;
      pendingVideoId = videoId;
      setNowPlaying(title, artist, url);

      if (player && typeof player.loadVideoById === "function") {{
        player.loadVideoById(videoId);
        player.playVideo();
        pendingVideoId = null;
      }}
    }}

    function pausePlayback() {{
      if (player && typeof player.pauseVideo === "function") {{
        player.pauseVideo();
      }}
    }}

    function resumePlayback() {{
      if (player && typeof player.playVideo === "function") {{
        player.playVideo();
      }}
    }}

    function stopPlayback() {{
      if (player && typeof player.stopVideo === "function") {{
        player.stopVideo();
      }}
    }}

    async function copyText(text) {{
      try {{
        await navigator.clipboard.writeText(text);
        setStatus("Copied URL to clipboard.");
      }} catch (err) {{
        setStatus("Clipboard copy failed.", "error");
      }}
    }}

    function openPreview(url) {{
      window.open(url, "_blank", "noopener,noreferrer");
    }}

    function onPlayerStateChange(event) {{
      const state = event.data;
      if (state === YT.PlayerState.PLAYING) {{
        setStatus("Playing.");
      }} else if (state === YT.PlayerState.PAUSED) {{
        setStatus("Paused.");
      }} else if (state === YT.PlayerState.ENDED) {{
        setStatus("Playback ended.");
      }}
    }}

    window.onYouTubeIframeAPIReady = function () {{
      player = new YT.Player("player", {{
        height: "100%",
        width: "100%",
        videoId: pendingVideoId || undefined,
        playerVars: {{
          playsinline: 1,
          rel: 0,
          modestbranding: 1,
          origin: window.location.origin,
        }},
        events: {{
          onReady: function () {{
            if (pendingVideoId) {{
              player.loadVideoById(pendingVideoId);
              player.playVideo();
              pendingVideoId = null;
            }}
          }},
          onStateChange: onPlayerStateChange,
        }},
      }});
    }};

    const apiScript = document.createElement("script");
    apiScript.src = "https://www.youtube.com/iframe_api";
    document.head.appendChild(apiScript);

    queryInput.addEventListener("keydown", (event) => {{
      if (event.key === "Enter") {{
        runSearch();
      }}
    }});

    urlInput.addEventListener("keydown", (event) => {{
      if (event.key === "Enter") {{
        playFromUrl();
      }}
    }});

    (async function bootstrap() {{
      if (INITIAL_QUERY) {{
        queryInput.value = INITIAL_QUERY;
        await runSearch();
      }}

      if (INITIAL_URL) {{
        urlInput.value = INITIAL_URL;
        await playFromUrl();
      }}
    }})();
  </script>
</body>
</html>"#
    )
}
