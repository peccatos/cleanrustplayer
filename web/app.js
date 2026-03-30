const state = {
  tracks: [],
  filteredTracks: [],
  currentTrackId: null
};

const els = {
  searchInput: document.getElementById("searchInput"),
  rescanBtn: document.getElementById("rescanBtn"),
  healthText: document.getElementById("healthText"),
  trackCountText: document.getElementById("trackCountText"),
  rootsText: document.getElementById("rootsText"),
  statusMessage: document.getElementById("statusMessage"),
  currentTitle: document.getElementById("currentTitle"),
  currentSubtitle: document.getElementById("currentSubtitle"),
  catalogInfo: document.getElementById("catalogInfo"),
  tracksList: document.getElementById("tracksList"),
  audioPlayer: document.getElementById("audioPlayer")
};

init();

function init() {
  bindEvents();
  void bootstrap();
}

function bindEvents() {
  els.searchInput.addEventListener("input", () => applyFilter(els.searchInput.value));
  els.rescanBtn.addEventListener("click", () => void rescanLibrary());
  els.tracksList.addEventListener("click", (event) => {
    const button = event.target.closest("[data-track-id]");
    if (!button) return;
    selectTrack(button.dataset.trackId);
  });
  els.audioPlayer.addEventListener("loadstart", () => setStatus("Backend готовит локальный stream endpoint.", "neutral"));
  els.audioPlayer.addEventListener("canplay", () => setStatus("Трек готов к воспроизведению.", "success"));
  els.audioPlayer.addEventListener("playing", () => setStatus("Играет через Rust backend.", "success"));
  els.audioPlayer.addEventListener("waiting", () => setStatus("Буферизация потока.", "neutral"));
  els.audioPlayer.addEventListener("error", () => setStatus("Не удалось прочитать поток. Проверь файл и серверный лог.", "error"));
}

async function bootstrap() {
  setStatus("Подключаюсь к backend и загружаю каталог.", "neutral");
  try {
    const [health, tracks] = await Promise.all([fetchJson("/api/health"), fetchJson("/api/tracks")]);
    renderHealth(health);
    setTracks(tracks.tracks || []);
    setStatus("Каталог загружен.", "success");
  } catch (error) {
    setStatus(error.message, "error");
    els.catalogInfo.textContent = "Backend не ответил.";
  }
}

async function rescanLibrary() {
  els.rescanBtn.disabled = true;
  setStatus("Сервер пересканирует музыкальные папки и обновит SQLite.", "neutral");
  try {
    const sync = await fetchJson("/api/library/rescan", { method: "POST" });
    const [health, tracks] = await Promise.all([fetchJson("/api/health"), fetchJson("/api/tracks")]);
    renderHealth(health);
    setTracks(tracks.tracks || []);
    setStatus(`Пересканировано ${sync.track_count} треков из ${sync.root_count} папок.`, "success");
  } catch (error) {
    setStatus(error.message, "error");
  } finally {
    els.rescanBtn.disabled = false;
  }
}

function setTracks(tracks) {
  state.tracks = tracks;
  applyFilter(els.searchInput.value);
}

function applyFilter(rawQuery) {
  const query = rawQuery.trim().toLowerCase();
  state.filteredTracks = state.tracks.filter((track) => {
    if (!query) return true;
    return [track.title, track.artist, track.album].join(" ").toLowerCase().includes(query);
  });
  renderTracks();
}

function renderHealth(health) {
  els.healthText.textContent = health.ok ? "Онлайн" : "Проблема";
  els.trackCountText.textContent = String(health.track_count ?? 0);
  els.rootsText.textContent = (health.roots || []).join(" | ") || "Нет путей";
}

function renderTracks() {
  els.tracksList.innerHTML = "";
  if (state.filteredTracks.length === 0) {
    els.catalogInfo.textContent = state.tracks.length === 0
      ? "Библиотека пустая. Проверь REPLAYCORE_LOCAL_MUSIC_ROOT(S) и нажми «Пересканировать»."
      : "По текущему фильтру ничего не найдено.";
    return;
  }

  els.catalogInfo.textContent = `Показано ${state.filteredTracks.length} из ${state.tracks.length} треков.`;
  const fragment = document.createDocumentFragment();

  for (const track of state.filteredTracks) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "track-card";
    button.dataset.trackId = track.id;
    button.setAttribute("aria-pressed", String(track.id === state.currentTrackId));

    const title = document.createElement("span");
    title.className = "track-card__title";
    title.textContent = track.title || "(без названия)";

    const subtitle = document.createElement("span");
    subtitle.className = "track-card__subtitle";
    subtitle.textContent = buildSubtitle(track);

    const meta = document.createElement("span");
    meta.className = "track-card__meta";
    meta.textContent = track.source_label || "Локальный файл";

    button.append(title, subtitle, meta);
    fragment.append(button);
  }

  els.tracksList.append(fragment);
}

function selectTrack(trackId) {
  const track = state.tracks.find((item) => item.id === trackId);
  if (!track) return;
  state.currentTrackId = trackId;
  els.currentTitle.textContent = track.title || "(без названия)";
  els.currentSubtitle.textContent = buildSubtitle(track);
  els.audioPlayer.src = track.stream_url;
  els.audioPlayer.load();
  renderTracks();
  setStatus("Трек выбран. Нажми Play на системном аудиоконтроле.", "neutral");
}

function buildSubtitle(track) {
  const parts = [];
  if (track.artist) parts.push(track.artist);
  if (track.album) parts.push(track.album);
  if (track.duration_ms) parts.push(formatDuration(track.duration_ms));
  return parts.join(" • ") || "Без метаданных";
}

function setStatus(message, tone) {
  els.statusMessage.textContent = message;
  els.statusMessage.dataset.tone = tone;
}

async function fetchJson(url, init) {
  const response = await fetch(url, {
    ...init,
    headers: { Accept: "application/json", ...(init?.headers || {}) }
  });

  const contentType = response.headers.get("content-type") || "";
  const payload = contentType.includes("application/json")
    ? await response.json()
    : { error: { message: await response.text() } };

  if (!response.ok) {
    throw new Error(payload?.error?.message || `HTTP ${response.status}`);
  }

  return payload;
}

function formatDuration(durationMs) {
  const totalSeconds = Math.round(durationMs / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}
