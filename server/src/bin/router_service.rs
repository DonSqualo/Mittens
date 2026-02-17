use std::collections::{HashMap, HashSet};
use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use axum::extract::{
    connect_info::ConnectInfo,
    ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
    Path as AxumPath, Query, State,
};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Json, Router};
use futures::{future::join_all, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};
use tokio_tungstenite::{connect_async, connect_async_with_config};
use tokio_tungstenite::tungstenite;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackendEntry {
    backend_id: String,
    ws_url: String,
    project_file: Option<String>,
    branch: Option<String>,
    owner: Option<String>,
    worktree: Option<String>,
    backend_port: Option<u16>,
    pid: Option<u32>,
    updated_at_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackendRegistry {
    updated_at_unix_ms: u64,
    backends: Vec<BackendEntry>,
}

#[derive(Debug, Deserialize, Default)]
struct WsRouteQuery {
    renderer_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct LiveConnection {
    connection_id: u64,
    renderer_id: String,
    backend_id: String,
    backend_ws_url: String,
    client_addr: String,
    connected_at_unix_ms: u64,
}

#[derive(Clone)]
struct RouterState {
    registry_path: PathBuf,
    live_connections: Arc<RwLock<HashMap<u64, LiveConnection>>>,
    next_connection_id: Arc<AtomicU64>,
}

#[derive(Debug, Serialize)]
struct RouterGraphResponse {
    generated_at_unix_ms: u64,
    registry_path: String,
    services: Vec<ServiceNode>,
    renderers: Vec<RendererNode>,
    backends: Vec<BackendNode>,
    edges: Vec<GraphEdge>,
    live_connections: Vec<LiveConnection>,
}

#[derive(Debug, Serialize)]
struct ServiceNode {
    kind: String,
    id: String,
    systemd_unit: String,
    configured_port: Option<u16>,
    listening_process: Option<String>,
    load_state: Option<String>,
    active_state: Option<String>,
    sub_state: Option<String>,
    unit_file_state: Option<String>,
    healthy: bool,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct RendererNode {
    renderer_id: String,
    systemd_unit: String,
    branch: Option<String>,
    worktree: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    live_connection_count: usize,
    listening_backend_ids: Vec<String>,
    client_addrs: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BackendNode {
    backend_id: String,
    ws_url: String,
    backend_port: Option<u16>,
    project_file: Option<String>,
    branch: Option<String>,
    owner: Option<String>,
    worktree: Option<String>,
    systemd_unit: String,
    live_connection_count: usize,
    listening_renderer_ids: Vec<String>,
    reachable: bool,
}

#[derive(Debug, Serialize)]
struct GraphEdge {
    renderer_id: String,
    backend_id: String,
    backend_ws_url: String,
    live_connection_count: usize,
    client_addrs: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Clone, Default)]
struct UnitStatus {
    load_state: Option<String>,
    active_state: Option<String>,
    sub_state: Option<String>,
    unit_file_state: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct StatusSnapshot {
    rows: Vec<StatusRow>,
}

#[derive(Debug, Deserialize, Clone)]
struct StatusRow {
    #[serde(rename = "kind")]
    _kind: String,
    unit: String,
    port: String,
    state: String,
    listen: String,
    process: String,
    exists: String,
}

#[derive(Debug, Clone, Default)]
struct RendererServiceConfig {
    renderer_id: String,
    systemd_unit: String,
    branch: Option<String>,
    worktree: Option<String>,
    host: Option<String>,
    port: Option<u16>,
}

#[derive(Default)]
struct RendererAgg {
    backend_ids: HashSet<String>,
    client_addrs: HashSet<String>,
    count: usize,
}

#[derive(Default)]
struct BackendAgg {
    renderer_ids: HashSet<String>,
    count: usize,
}

#[derive(Default)]
struct EdgeAgg {
    backend_ws_url: String,
    count: usize,
    client_addrs: HashSet<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let registry_path = resolve_registry_path();
    let listen_port = resolve_port()?;

    info!("router registry path: {}", registry_path.display());
    info!("router listen port: {}", listen_port);

    let state = Arc::new(RouterState {
        registry_path,
        live_connections: Arc::new(RwLock::new(HashMap::new())),
        next_connection_id: Arc::new(AtomicU64::new(1)),
    });

    let app = Router::new()
        .route("/api/backends", get(list_backends))
        .route("/api/graph", get(router_graph))
        .route("/graph", get(graph_page))
        .route("/ws/:backend_id", get(proxy_backend_ws))
        .route("/healthz", get(healthz))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", listen_port)).await?;
    info!("router service listening on http://0.0.0.0:{listen_port}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

fn resolve_registry_path() -> PathBuf {
    if let Ok(path) = env::var("MITTENS_REGISTRY_PATH") {
        return PathBuf::from(path);
    }
    default_registry_path()
}

fn default_registry_path() -> PathBuf {
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(".mittens").join("backends.json");
    }
    PathBuf::from(".mittens/backends.json")
}

fn resolve_port() -> Result<u16> {
    if let Ok(port_str) = env::var("MITTENS_ROUTER_PORT") {
        return port_str
            .parse::<u16>()
            .map_err(|e| anyhow!("invalid MITTENS_ROUTER_PORT '{port_str}': {e}"));
    }
    Ok(3100)
}

async fn healthz() -> impl IntoResponse {
    "ok"
}

async fn list_backends(State(state): State<Arc<RouterState>>) -> impl IntoResponse {
    match load_registry(&state.registry_path).await {
        Ok(mut registry) => {
            registry
                .backends
                .sort_by(|a, b| a.backend_id.cmp(&b.backend_id));
            Json(registry).into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: err.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn router_graph(State(state): State<Arc<RouterState>>) -> impl IntoResponse {
    let mut registry = match load_registry(&state.registry_path).await {
        Ok(registry) => registry,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("failed to load registry: {err}"),
                }),
            )
                .into_response();
        }
    };
    registry
        .backends
        .sort_by(|a, b| a.backend_id.cmp(&b.backend_id));

    let live_connections = {
        let connections = state.live_connections.read().await;
        let mut values: Vec<LiveConnection> = connections.values().cloned().collect();
        values.sort_by(|a, b| a.connection_id.cmp(&b.connection_id));
        values
    };

    let graph = build_graph_response(&state.registry_path, registry, &live_connections).await;
    Json(graph).into_response()
}

async fn graph_page() -> Html<&'static str> {
    Html(GRAPH_PAGE_HTML)
}

const GRAPH_PAGE_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Mittens</title>
  <style>
    :root {
      --bg: #000000;
      --panel: #060606;
      --line: #1f1f1f;
      --text: #e8e8e8;
      --muted: #8e8e8e;
      --renderer: #74b285;
      --backend: #7895ad;
      --down: #df4f4f;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      background: var(--bg);
      color: var(--text);
      font-family: "IBM Plex Mono", "JetBrains Mono", monospace;
      position: relative;
    }
    body::before {
      content: "";
      position: fixed;
      inset: 0;
      pointer-events: none;
      opacity: 0.03;
      background-image: url("data:image/svg+xml,%3Csvg viewBox='0 0 200 200' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='4' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E");
    }
    .layout {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 8px;
      height: 100vh;
      padding: 12px;
      position: relative;
      z-index: 1;
    }
    @media (max-width: 980px) {
      .layout {
        grid-template-columns: 1fr;
        height: auto;
        min-height: 100vh;
      }
    }
    .column-wrap {
      display: flex;
      flex-direction: column;
      gap: 8px;
      min-height: 0;
    }
    .column-header {
      color: #f2f2f2;
      font-size: 12px;
      font-weight: 700;
      letter-spacing: 0.7px;
      text-transform: uppercase;
      background: transparent;
      padding: 8px 10px;
    }
    .column {
      border: 1px solid var(--line);
      background: var(--panel);
      padding: 8px;
      min-height: 0;
      flex: 1;
      overflow: auto;
    }
    .cards {
      display: grid;
      gap: 8px;
    }
    .card {
      width: 100%;
      text-align: left;
      border: 1px solid var(--line);
      background: transparent;
      color: var(--text);
      padding: 8px;
      font: inherit;
      cursor: pointer;
      text-decoration: none;
      display: block;
    }
    .card:hover {
      border-color: #424242;
    }
    .card.active-renderer {
      border-color: var(--renderer);
      background: rgba(116, 178, 133, 0.12);
      color: #d3e7d9;
    }
    .card.active-backend {
      border-color: var(--backend);
      background: rgba(120, 149, 173, 0.12);
      color: #dae4eb;
    }
    .card.down-backend {
      border-color: #8a2f2f;
      background: rgba(160, 45, 45, 0.16);
    }
    .card.down-backend .title {
      color: #ff8f8f;
    }
    .card.active-backend.down-backend {
      border-color: var(--down);
      background: rgba(223, 79, 79, 0.22);
      color: #ffd6d6;
    }
    .card.key-focus {
      box-shadow: inset 0 0 0 1px #e5e5e5;
    }
    .card.disabled {
      opacity: 0.55;
      cursor: not-allowed;
    }
    .title {
      font-size: 12px;
      font-weight: 700;
      margin-bottom: 3px;
      color: #fff;
    }
    .meta {
      color: var(--muted);
      font-size: 10px;
      line-height: 1.35;
      word-break: break-word;
    }
    .empty {
      border: 1px dashed var(--line);
      color: var(--muted);
      padding: 12px;
      font-size: 11px;
      text-align: center;
    }
  </style>
</head>
<body>
  <main class="layout">
    <div class="column-wrap">
      <div class="column-header">Renderer</div>
      <section class="column">
        <div id="renderers" class="cards"></div>
      </section>
    </div>
    <div class="column-wrap">
      <div class="column-header">Server</div>
      <section class="column">
        <div id="backends" class="cards"></div>
      </section>
    </div>
  </main>
  <script>
    const query = new URLSearchParams(window.location.search);
    const focusBackend = query.get('focus_backend');
    const requestedRenderer = query.get('renderer_id');
    let selectedRendererId = requestedRenderer || null;
    let selectedBackendId = focusBackend || null;
    let latestGraph = null;
    let focusedColumn = focusBackend ? 'backend' : 'renderer';
    let rendererCursor = 0;
    let backendCursor = 0;

    function esc(value) {
      return String(value ?? '')
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;');
    }

    function shortPath(raw) {
      if (!raw) return '-';
      const parts = String(raw).split('/');
      return parts[parts.length - 1] || raw;
    }

    function rendererById(renderers, rendererId) {
      return (renderers || []).find((r) => r.renderer_id === rendererId) || null;
    }

    function rendererUrl(renderer, backendId) {
      if (!renderer || !renderer.port || !backendId) return '';
      return window.location.protocol + '//' + window.location.hostname + ':' + renderer.port + '/?backend_id=' + encodeURIComponent(backendId);
    }

    function backendPortLabel(backend) {
      if (backend && backend.backend_port) {
        return String(backend.backend_port);
      }
      const ws = String((backend && backend.ws_url) || '');
      const match = ws.match(/:(\d+)(?:\/|$)/);
      return match ? match[1] : '-';
    }

    function persistSelectionToQuery() {
      const next = new URLSearchParams(window.location.search);
      if (selectedRendererId) next.set('renderer_id', selectedRendererId);
      else next.delete('renderer_id');
      if (selectedBackendId) next.set('focus_backend', selectedBackendId);
      else next.delete('focus_backend');
      const nextUrl = window.location.pathname + (next.toString() ? ('?' + next.toString()) : '');
      window.history.replaceState(null, '', nextUrl);
    }

    function clampIndex(index, length) {
      if (!length || length <= 0) return 0;
      return Math.max(0, Math.min(index, length - 1));
    }

    function isTypingContext() {
      const active = document.activeElement;
      if (!active) return false;
      const tag = active.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
      return Boolean(active.isContentEditable);
    }

    function currentRenderers() {
      return (latestGraph && latestGraph.renderers) || [];
    }

    function currentBackends() {
      return (latestGraph && latestGraph.backends) || [];
    }

    function syncRendererCursor(renderers) {
      if (!renderers || renderers.length === 0) {
        rendererCursor = 0;
        selectedRendererId = null;
        return;
      }
      const selectedIndex = renderers.findIndex((r) => r.renderer_id === selectedRendererId);
      if (selectedIndex >= 0) {
        rendererCursor = selectedIndex;
        return;
      }
      rendererCursor = clampIndex(rendererCursor, renderers.length);
      selectedRendererId = renderers[rendererCursor].renderer_id;
    }

    function syncBackendCursor(backends) {
      if (!backends || backends.length === 0) {
        backendCursor = 0;
        selectedBackendId = null;
        return;
      }
      const selectedIndex = backends.findIndex((b) => b.backend_id === selectedBackendId);
      if (selectedIndex >= 0) {
        backendCursor = selectedIndex;
        return;
      }
      backendCursor = clampIndex(backendCursor, backends.length);
    }

    function applyKeyboardFocus() {
      const rendererCards = Array.from(document.querySelectorAll('#renderers [data-renderer-id]'));
      const backendCards = Array.from(document.querySelectorAll('#backends [data-backend-id]'));
      rendererCards.forEach((el) => el.classList.remove('key-focus'));
      backendCards.forEach((el) => el.classList.remove('key-focus'));

      if (focusedColumn === 'backend') {
        if (backendCards.length === 0) return;
        backendCursor = clampIndex(backendCursor, backendCards.length);
        const target = backendCards[backendCursor];
        target.classList.add('key-focus');
        target.scrollIntoView({ block: 'nearest' });
        return;
      }

      if (rendererCards.length === 0) return;
      rendererCursor = clampIndex(rendererCursor, rendererCards.length);
      const target = rendererCards[rendererCursor];
      target.classList.add('key-focus');
      target.scrollIntoView({ block: 'nearest' });
    }

    function openSelectedBackend() {
      const backends = currentBackends();
      if (!backends || backends.length === 0) return;
      syncBackendCursor(backends);
      const backend = backends[backendCursor];
      if (!backend) return;

      selectedBackendId = backend.backend_id;
      persistSelectionToQuery();
      render(latestGraph);

      const renderer = rendererById(currentRenderers(), selectedRendererId);
      const url = rendererUrl(renderer, backend.backend_id);
      if (!url) return;
      window.location.href = url;
    }

    function moveCursor(delta) {
      if (focusedColumn === 'backend') {
        const backends = currentBackends();
        if (!backends || backends.length === 0) return;
        syncBackendCursor(backends);
        backendCursor = clampIndex(backendCursor + delta, backends.length);
        selectedBackendId = backends[backendCursor].backend_id;
        persistSelectionToQuery();
        render(latestGraph);
        return;
      }

      const renderers = currentRenderers();
      if (!renderers || renderers.length === 0) return;
      syncRendererCursor(renderers);
      rendererCursor = clampIndex(rendererCursor + delta, renderers.length);
      selectedRendererId = renderers[rendererCursor].renderer_id;
      persistSelectionToQuery();
      render(latestGraph);
    }

    function renderRenderers(renderers) {
      const root = document.getElementById('renderers');
      if (!renderers || renderers.length === 0) {
        root.innerHTML = '<div class="empty">No renderers</div>';
        syncRendererCursor([]);
        applyKeyboardFocus();
        return;
      }

      syncRendererCursor(renderers);

      root.innerHTML = renderers.map((r) => {
        const selected = r.renderer_id === selectedRendererId;
        const selectedClass = selected ? ' active-renderer' : '';
        return '<button class="card' + selectedClass + '" data-renderer-id="' + esc(r.renderer_id) + '">' +
          '<div class="title">' + esc(r.renderer_id) + '</div>' +
          '<div class="meta">🔌 :' + esc(r.port || '-') + ' · 🟢 ' + esc(r.live_connection_count || 0) + ' live</div>' +
          '<div class="meta">🌿 ' + esc(r.branch || '-') + ' · 📁 ' + esc(shortPath(r.worktree || '-')) + '</div>' +
        '</button>';
      }).join('');

      root.querySelectorAll('[data-renderer-id]').forEach((el) => {
        el.addEventListener('click', () => {
          selectedRendererId = el.getAttribute('data-renderer-id');
          const idx = renderers.findIndex((r) => r.renderer_id === selectedRendererId);
          if (idx >= 0) rendererCursor = idx;
          focusedColumn = 'renderer';
          persistSelectionToQuery();
          render(latestGraph);
        });
      });
    }

    function renderBackends(backends, renderers) {
      const root = document.getElementById('backends');
      if (!backends || backends.length === 0) {
        root.innerHTML = '<div class="empty">No backends</div>';
        syncBackendCursor([]);
        applyKeyboardFocus();
      } else {
        syncBackendCursor(backends);
        root.innerHTML = backends.map((b) => {
          const selectedRenderer = rendererById(renderers, selectedRendererId);
          const canOpen = Boolean(selectedRenderer && selectedRenderer.port);
          const active = b.backend_id === selectedBackendId;
          const reachable = b.reachable !== false;
          const selectedClass = active ? ' active-backend' : '';
          const downClass = reachable ? '' : ' down-backend';
          const disabledClass = canOpen ? '' : ' disabled';
          const targetUrl = canOpen ? rendererUrl(selectedRenderer, b.backend_id) : '#';
          const healthLabel = reachable ? '🟢 up' : '🔴 down';
          return '<a class="card' + selectedClass + downClass + disabledClass + '" data-backend-id="' + esc(b.backend_id) + '" data-can-open="' + (canOpen ? '1' : '0') + '" href="' + esc(targetUrl) + '">' +
            '<div class="title">' + esc(b.backend_id) + '</div>' +
            '<div class="meta">🔌 :' + esc(backendPortLabel(b)) + ' · ' + healthLabel + ' · 🟢 ' + esc(b.live_connection_count || 0) + ' live</div>' +
            '<div class="meta">🌿 ' + esc(b.branch || '-') + ' · 📁 ' + esc(shortPath(b.project_file)) + '</div>' +
          '</a>';
        }).join('');
      }

      root.querySelectorAll('[data-backend-id]').forEach((el) => {
        el.addEventListener('click', (event) => {
          event.preventDefault();
          const backendId = el.getAttribute('data-backend-id');
          if (!backendId) return;
          selectedBackendId = backendId;
          const idx = backends.findIndex((b) => b.backend_id === backendId);
          if (idx >= 0) backendCursor = idx;
          focusedColumn = 'backend';
          persistSelectionToQuery();
          render(latestGraph);
          const canOpen = el.getAttribute('data-can-open') === '1';
          const currentRenderer = rendererById((latestGraph && latestGraph.renderers) || [], selectedRendererId);
          const url = rendererUrl(currentRenderer, backendId);
          if (!canOpen || !url) {
            return;
          }
          window.location.href = url;
        });
      });
    }

    function render(data) {
      if (!data) return;
      latestGraph = data;

      const renderers = data.renderers || [];
      const backends = data.backends || [];
      renderRenderers(renderers);
      renderBackends(backends, renderers);
      applyKeyboardFocus();
    }

    document.addEventListener('keydown', (event) => {
      if (isTypingContext()) return;

      const key = event.key;
      const lower = key.length === 1 ? key.toLowerCase() : key;

      if ((event.ctrlKey || event.metaKey) && lower === 'k') {
        event.preventDefault();
        window.location.href = '/graph' + window.location.search;
        return;
      }

      if (!event.ctrlKey && !event.metaKey && (lower === 'h' || key === 'ArrowLeft')) {
        event.preventDefault();
        focusedColumn = 'renderer';
        applyKeyboardFocus();
        return;
      }

      if (!event.ctrlKey && !event.metaKey && (lower === 'l' || key === 'ArrowRight')) {
        event.preventDefault();
        focusedColumn = 'backend';
        applyKeyboardFocus();
        return;
      }

      if (!event.ctrlKey && !event.metaKey && (lower === 'j' || key === 'ArrowDown')) {
        event.preventDefault();
        moveCursor(1);
        return;
      }

      if (!event.ctrlKey && !event.metaKey && (lower === 'k' || key === 'ArrowUp')) {
        event.preventDefault();
        moveCursor(-1);
        return;
      }

      if (key === 'Enter') {
        event.preventDefault();
        if (focusedColumn !== 'backend') {
          focusedColumn = 'backend';
          applyKeyboardFocus();
          return;
        }
        openSelectedBackend();
      }
    });

    let refreshInFlight = { active: false };

    async function refresh() {
      if (refreshInFlight.active) return;
      refreshInFlight.active = true;
      try {
        const res = await fetch('/api/graph', { cache: 'no-store' });
        if (!res.ok) throw new Error('HTTP ' + res.status);
        const data = await res.json();
        render(data);
      } catch (err) {
        document.getElementById('renderers').innerHTML = '<div class="empty">Failed to load</div>';
        document.getElementById('backends').innerHTML = '<div class="empty">' + esc(err) + '</div>';
      } finally {
        refreshInFlight.active = false;
      }
    }

    refresh();
    setInterval(refresh, 5000);
  </script>
</body>
</html>
"#;

async fn proxy_backend_ws(
    AxumPath(backend_id): AxumPath<String>,
    Query(query): Query<WsRouteQuery>,
    ConnectInfo(client_addr): ConnectInfo<SocketAddr>,
    ws: WebSocketUpgrade,
    State(state): State<Arc<RouterState>>,
) -> impl IntoResponse {
    let registry = match load_registry(&state.registry_path).await {
        Ok(registry) => registry,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("failed to load registry: {err}"),
                }),
            )
                .into_response();
        }
    };

    let backend = match registry
        .backends
        .iter()
        .find(|entry| entry.backend_id == backend_id)
    {
        Some(entry) => entry.clone(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("backend_id '{}' not found", backend_id),
                }),
            )
                .into_response();
        }
    };

    if !is_ws_url(&backend.ws_url) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!(
                    "backend '{}' has invalid ws_url '{}'",
                    backend_id, backend.ws_url
                ),
            }),
        )
            .into_response();
    }

    let target_url = backend.ws_url.clone();
    let renderer_id = normalize_renderer_id(query.renderer_id, client_addr);
    let connection_id = state.next_connection_id.fetch_add(1, Ordering::Relaxed);
    let client_addr_text = client_addr.to_string();
    let ws_state = state.clone();
    ws.on_upgrade(move |socket| {
        proxy_ws_connection(
            ws_state,
            connection_id,
            backend_id,
            renderer_id,
            client_addr_text,
            target_url,
            socket,
        )
    })
    .into_response()
}

fn is_ws_url(ws_url: &str) -> bool {
    ws_url.starts_with("ws://") || ws_url.starts_with("wss://")
}

async fn detect_backend_reachability(backends: &[BackendEntry]) -> HashMap<String, bool> {
    let probes = backends.iter().map(|backend| {
        let backend_id = backend.backend_id.clone();
        let ws_url = backend.ws_url.clone();
        async move {
            let reachable = probe_backend_ws_url(&ws_url).await;
            (backend_id, reachable)
        }
    });

    join_all(probes).await.into_iter().collect()
}

async fn probe_backend_ws_url(ws_url: &str) -> bool {
    if !is_ws_url(ws_url) {
        return false;
    }

    let connect_result = timeout(Duration::from_millis(700), connect_async(ws_url)).await;
    match connect_result {
        Ok(Ok((mut socket, _response))) => {
            let _ = timeout(Duration::from_millis(150), socket.close(None)).await;
            true
        }
        _ => false,
    }
}

async fn load_registry(path: &Path) -> Result<BackendRegistry> {
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            let registry = serde_json::from_slice::<BackendRegistry>(&bytes)?;
            Ok(registry)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(BackendRegistry {
            updated_at_unix_ms: 0,
            backends: Vec::new(),
        }),
        Err(err) => Err(err.into()),
    }
}

async fn proxy_ws_connection(
    state: Arc<RouterState>,
    connection_id: u64,
    backend_id: String,
    renderer_id: String,
    client_addr: String,
    target_url: String,
    client_socket: WebSocket,
) {
    info!(
        "proxy connect renderer_id='{}' backend_id='{}' target='{}' client='{}'",
        renderer_id, backend_id, target_url, client_addr
    );

    // Large CAD payloads can exceed tungstenite's default max_frame_size (16 MiB).
    // If we keep the default, the router will disconnect as soon as a large mesh arrives.
    let max_frame_mb: usize = std::env::var("MITTENS_ROUTER_MAX_WS_FRAME_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(128);
    let max_message_mb: usize = std::env::var("MITTENS_ROUTER_MAX_WS_MESSAGE_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(256);
    let mut ws_cfg = tungstenite::protocol::WebSocketConfig::default();
    ws_cfg.max_frame_size = Some(max_frame_mb.saturating_mul(1024 * 1024));
    ws_cfg.max_message_size = Some(max_message_mb.saturating_mul(1024 * 1024));

    let upstream = connect_async_with_config(&target_url, Some(ws_cfg), false).await;
    let (upstream_socket, _) = match upstream {
        Ok(tuple) => tuple,
        Err(err) => {
            error!(
                "proxy connect failed renderer_id='{}' backend_id='{}' target='{}': {}",
                renderer_id, backend_id, target_url, err
            );
            return;
        }
    };

    {
        let mut connections = state.live_connections.write().await;
        connections.insert(
            connection_id,
            LiveConnection {
                connection_id,
                renderer_id: renderer_id.clone(),
                backend_id: backend_id.clone(),
                backend_ws_url: target_url.clone(),
                client_addr: client_addr.clone(),
                connected_at_unix_ms: now_unix_ms(),
            },
        );
    }

    let (mut client_sender, mut client_receiver) = client_socket.split();
    let (mut upstream_sender, mut upstream_receiver) = upstream_socket.split();

    let upstream_to_client = async {
        while let Some(message_result) = upstream_receiver.next().await {
            let message = match message_result {
                Ok(message) => message,
                Err(err) => {
                    error!(
                        "upstream read error renderer_id='{}' backend_id='{}': {}",
                        renderer_id, backend_id, err
                    );
                    break;
                }
            };

            match tungstenite_to_axum(message) {
                Some(client_message) => {
                    if client_sender.send(client_message).await.is_err() {
                        break;
                    }
                }
                None => break,
            }
        }
    };

    let client_to_upstream = async {
        while let Some(message_result) = client_receiver.next().await {
            let message = match message_result {
                Ok(message) => message,
                Err(err) => {
                    error!(
                        "client read error renderer_id='{}' backend_id='{}': {}",
                        renderer_id, backend_id, err
                    );
                    break;
                }
            };

            match axum_to_tungstenite(message) {
                Some(upstream_message) => {
                    if upstream_sender.send(upstream_message).await.is_err() {
                        break;
                    }
                }
                None => break,
            }
        }
    };

    tokio::select! {
        _ = upstream_to_client => {}
        _ = client_to_upstream => {}
    }

    {
        let mut connections = state.live_connections.write().await;
        connections.remove(&connection_id);
    }

    info!(
        "proxy disconnect renderer_id='{}' backend_id='{}'",
        renderer_id, backend_id
    );
}

async fn build_graph_response(
    registry_path: &Path,
    registry: BackendRegistry,
    live_connections: &[LiveConnection],
) -> RouterGraphResponse {
    let renderer_configs = discover_renderer_service_configs();
    let mut renderer_config_by_id: HashMap<String, RendererServiceConfig> = HashMap::new();
    for config in renderer_configs {
        renderer_config_by_id.insert(config.renderer_id.clone(), config);
    }

    let mut renderer_ids: HashSet<String> = renderer_config_by_id.keys().cloned().collect();
    for connection in live_connections {
        renderer_ids.insert(connection.renderer_id.clone());
    }

    let mut renderer_agg: HashMap<String, RendererAgg> = HashMap::new();
    let mut backend_agg: HashMap<String, BackendAgg> = HashMap::new();
    let mut edge_agg: HashMap<(String, String), EdgeAgg> = HashMap::new();

    for connection in live_connections {
        let renderer = renderer_agg
            .entry(connection.renderer_id.clone())
            .or_default();
        renderer.count += 1;
        renderer.backend_ids.insert(connection.backend_id.clone());
        renderer.client_addrs.insert(connection.client_addr.clone());

        let backend = backend_agg
            .entry(connection.backend_id.clone())
            .or_default();
        backend.count += 1;
        backend.renderer_ids.insert(connection.renderer_id.clone());

        let edge = edge_agg
            .entry((
                connection.renderer_id.clone(),
                connection.backend_id.clone(),
            ))
            .or_default();
        edge.backend_ws_url = connection.backend_ws_url.clone();
        edge.count += 1;
        edge.client_addrs.insert(connection.client_addr.clone());
    }

    let mut renderers = Vec::new();
    let mut sorted_renderer_ids: Vec<String> = renderer_ids.into_iter().collect();
    sorted_renderer_ids.sort();
    for renderer_id in sorted_renderer_ids {
        let config = renderer_config_by_id.get(&renderer_id);
        let agg = renderer_agg.remove(&renderer_id).unwrap_or_default();
        let mut backend_ids: Vec<String> = agg.backend_ids.into_iter().collect();
        backend_ids.sort();
        let mut client_addrs: Vec<String> = agg.client_addrs.into_iter().collect();
        client_addrs.sort();

        let systemd_unit = config
            .map(|cfg| cfg.systemd_unit.clone())
            .unwrap_or_else(|| renderer_unit_name(&renderer_id));

        renderers.push(RendererNode {
            renderer_id,
            systemd_unit,
            branch: config.and_then(|cfg| cfg.branch.clone()),
            worktree: config.and_then(|cfg| cfg.worktree.clone()),
            host: config.and_then(|cfg| cfg.host.clone()),
            port: config.and_then(|cfg| cfg.port),
            live_connection_count: agg.count,
            listening_backend_ids: backend_ids,
            client_addrs,
        });
    }

    let backend_reachability = detect_backend_reachability(&registry.backends).await;

    let mut backends = Vec::new();
    for backend in &registry.backends {
        let agg = backend_agg.remove(&backend.backend_id).unwrap_or_default();
        let mut renderer_ids: Vec<String> = agg.renderer_ids.into_iter().collect();
        renderer_ids.sort();
        backends.push(BackendNode {
            backend_id: backend.backend_id.clone(),
            ws_url: backend.ws_url.clone(),
            backend_port: backend.backend_port,
            project_file: backend.project_file.clone(),
            branch: backend.branch.clone(),
            owner: backend.owner.clone(),
            worktree: backend.worktree.clone(),
            systemd_unit: backend_unit_name(&backend.backend_id),
            live_connection_count: agg.count,
            listening_renderer_ids: renderer_ids,
            reachable: backend_reachability
                .get(&backend.backend_id)
                .copied()
                .unwrap_or(false),
        });
    }
    backends.sort_by(|a, b| a.backend_id.cmp(&b.backend_id));

    let mut edges: Vec<GraphEdge> = edge_agg
        .into_iter()
        .map(|((renderer_id, backend_id), agg)| {
            let mut client_addrs: Vec<String> = agg.client_addrs.into_iter().collect();
            client_addrs.sort();
            GraphEdge {
                renderer_id,
                backend_id,
                backend_ws_url: agg.backend_ws_url,
                live_connection_count: agg.count,
                client_addrs,
            }
        })
        .collect();
    edges.sort_by(|a, b| {
        a.renderer_id
            .cmp(&b.renderer_id)
            .then(a.backend_id.cmp(&b.backend_id))
    });

    let status_rows = query_status_rows_from_mittens().await;
    let mut status_rows_by_unit: HashMap<String, StatusRow> = HashMap::new();
    for row in status_rows {
        status_rows_by_unit.insert(row.unit.clone(), row);
    }

    let mut services = Vec::new();
    services.push(
        service_node(
            "router",
            "router",
            "mittens-router.service",
            status_rows_by_unit.get("mittens-router.service"),
        )
        .await,
    );
    for backend in &backends {
        services.push(
            service_node(
                "backend",
                &backend.backend_id,
                &backend.systemd_unit,
                status_rows_by_unit.get(&backend.systemd_unit),
            )
            .await,
        );
    }
    for renderer in &renderers {
        services.push(
            service_node(
                "renderer",
                &renderer.renderer_id,
                &renderer.systemd_unit,
                status_rows_by_unit.get(&renderer.systemd_unit),
            )
            .await,
        );
    }
    services.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.id.cmp(&b.id)));

    RouterGraphResponse {
        generated_at_unix_ms: now_unix_ms(),
        registry_path: registry_path.display().to_string(),
        services,
        renderers,
        backends,
        edges,
        live_connections: live_connections.to_vec(),
    }
}

async fn service_node(
    kind: &str,
    id: &str,
    systemd_unit: &str,
    status_row: Option<&StatusRow>,
) -> ServiceNode {
    if let Some(row) = status_row {
        let (active_state, sub_state) = parse_state_parts(&row.state);
        let configured_port = row.port.parse::<u16>().ok();
        let listening_process = non_dash(&row.process);
        let healthy = row.listen == "yes";
        return ServiceNode {
            kind: kind.to_string(),
            id: id.to_string(),
            systemd_unit: systemd_unit.to_string(),
            configured_port,
            listening_process,
            load_state: Some(if row.exists == "yes" {
                "loaded".to_string()
            } else {
                "not-found".to_string()
            }),
            active_state,
            sub_state,
            unit_file_state: Some(if row.exists == "yes" {
                "enabled".to_string()
            } else {
                "missing".to_string()
            }),
            healthy,
            error: None,
        };
    }

    let status = query_systemd_unit_status(systemd_unit).await;
    let healthy = status.healthy();
    let configured_port = configured_port_for_unit(systemd_unit);
    let listening_process = configured_port.and_then(listening_process_for_port);
    ServiceNode {
        kind: kind.to_string(),
        id: id.to_string(),
        systemd_unit: systemd_unit.to_string(),
        configured_port,
        listening_process,
        load_state: status.load_state,
        active_state: status.active_state,
        sub_state: status.sub_state,
        unit_file_state: status.unit_file_state,
        healthy,
        error: status.error,
    }
}

async fn query_status_rows_from_mittens() -> Vec<StatusRow> {
    let output_result = timeout(
        Duration::from_millis(1600),
        Command::new("./mittens").arg("status-json").output(),
    )
    .await;

    let output = match output_result {
        Ok(Ok(output)) if output.status.success() => output,
        _ => return Vec::new(),
    };

    serde_json::from_slice::<StatusSnapshot>(&output.stdout)
        .map(|snapshot| snapshot.rows)
        .unwrap_or_default()
}

fn parse_state_parts(state: &str) -> (Option<String>, Option<String>) {
    if let Some((active, sub)) = state.split_once('/') {
        return (non_dash(active), non_dash(sub));
    }
    (non_dash(state), None)
}

fn non_dash(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        None
    } else {
        Some(trimmed.to_string())
    }
}

async fn query_systemd_unit_status(unit_name: &str) -> UnitStatus {
    let output_result = timeout(
        Duration::from_millis(1200),
        Command::new("systemctl")
            .arg("--user")
            .arg("show")
            .arg(unit_name)
            .arg("--property=LoadState")
            .arg("--property=ActiveState")
            .arg("--property=SubState")
            .arg("--property=UnitFileState")
            .arg("--value")
            .output(),
    )
    .await;

    let output = match output_result {
        Err(_) => {
            return UnitStatus {
                error: Some("systemctl query timed out".to_string()),
                ..UnitStatus::default()
            };
        }
        Ok(Err(err)) => {
            return UnitStatus {
                error: Some(format!("failed to execute systemctl: {err}")),
                ..UnitStatus::default()
            };
        }
        Ok(Ok(output)) => output,
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let error = if stderr.is_empty() {
            format!("systemctl exited with status {}", output.status)
        } else {
            stderr
        };
        return UnitStatus {
            error: Some(error),
            ..UnitStatus::default()
        };
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    UnitStatus {
        load_state: non_empty(lines.next()),
        active_state: non_empty(lines.next()),
        sub_state: non_empty(lines.next()),
        unit_file_state: non_empty(lines.next()),
        error: None,
    }
}

fn configured_port_for_unit(unit_name: &str) -> Option<u16> {
    let output = StdCommand::new("systemctl")
        .arg("--user")
        .arg("show")
        .arg(unit_name)
        .arg("--property=ExecStart")
        .arg("--value")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let exec_start = String::from_utf8_lossy(&output.stdout);
    let tokens: Vec<&str> = exec_start.split_whitespace().collect();
    extract_flag_value(&tokens, "--port").and_then(|p| p.parse::<u16>().ok())
}

fn listening_process_for_port(port: u16) -> Option<String> {
    let output = StdCommand::new("ss")
        .arg("-ltnp")
        .arg(format!("sport = :{}", port))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().skip(1) {
        if let Some(proc_part) = line.split("users:(").nth(1) {
            return Some(proc_part.trim_end_matches(')').to_string());
        }
    }
    None
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

impl UnitStatus {
    fn healthy(&self) -> bool {
        let active_ok = matches!(self.active_state.as_deref(), Some("active"));
        let sub_ok = matches!(
            self.sub_state.as_deref(),
            Some("running") | Some("listening")
        );
        active_ok && sub_ok
    }
}

fn normalize_renderer_id(raw_renderer_id: Option<String>, client_addr: SocketAddr) -> String {
    if let Some(raw) = raw_renderer_id {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    format!("anonymous@{}", client_addr.ip())
}

fn discover_renderer_service_configs() -> Vec<RendererServiceConfig> {
    let mut configs = Vec::new();
    let unit_dir = systemd_user_dir();
    let entries = match std::fs::read_dir(&unit_dir) {
        Ok(entries) => entries,
        Err(_) => return configs,
    };

    for entry_result in entries {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        let unit_name = match path.file_name().and_then(|name| name.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        if !unit_name.starts_with("mittens-renderer-") || !unit_name.ends_with(".service") {
            continue;
        }

        let fallback_renderer_id = unit_name
            .trim_start_matches("mittens-renderer-")
            .trim_end_matches(".service")
            .to_string();
        let mut config = RendererServiceConfig {
            renderer_id: fallback_renderer_id,
            systemd_unit: unit_name,
            branch: None,
            worktree: None,
            host: None,
            port: None,
        };

        if let Ok(contents) = std::fs::read_to_string(path) {
            if let Some(exec_start) = extract_exec_start_line(&contents) {
                let tokens: Vec<&str> = exec_start.split_whitespace().collect();
                if let Some(renderer_id) = extract_flag_value(&tokens, "--renderer-id") {
                    config.renderer_id = renderer_id;
                }
                config.branch = extract_flag_value(&tokens, "--branch");
                config.worktree = extract_flag_value(&tokens, "--worktree");
                config.host = extract_flag_value(&tokens, "--host");
                config.port =
                    extract_flag_value(&tokens, "--port").and_then(|raw| raw.parse::<u16>().ok());
            }
        }
        if config.branch.is_none() {
            config.branch = config.worktree.as_deref().and_then(detect_git_branch);
        }

        configs.push(config);
    }

    configs.sort_by(|a, b| a.renderer_id.cmp(&b.renderer_id));
    configs
}

fn extract_exec_start_line(unit_contents: &str) -> Option<String> {
    for line in unit_contents.lines() {
        if let Some(rest) = line.trim().strip_prefix("ExecStart=") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

fn extract_flag_value(tokens: &[&str], flag: &str) -> Option<String> {
    for window in tokens.windows(2) {
        if window[0] == flag {
            return Some(window[1].to_string());
        }
    }
    None
}

fn detect_git_branch(worktree: &str) -> Option<String> {
    let output = StdCommand::new("git")
        .arg("-C")
        .arg(worktree)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        None
    } else {
        Some(branch)
    }
}

fn backend_unit_name(backend_id: &str) -> String {
    format!("mittens-backend-{}.service", sanitize_unit_id(backend_id))
}

fn renderer_unit_name(renderer_id: &str) -> String {
    format!("mittens-renderer-{}.service", sanitize_unit_id(renderer_id))
}

fn sanitize_unit_id(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '@' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn systemd_user_dir() -> PathBuf {
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("systemd")
            .join("user");
    }
    PathBuf::from(".config/systemd/user")
}

fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn axum_to_tungstenite(message: Message) -> Option<tungstenite::Message> {
    match message {
        Message::Text(text) => Some(tungstenite::Message::Text(text)),
        Message::Binary(bytes) => Some(tungstenite::Message::Binary(bytes.to_vec())),
        Message::Ping(bytes) => Some(tungstenite::Message::Ping(bytes.to_vec())),
        Message::Pong(bytes) => Some(tungstenite::Message::Pong(bytes.to_vec())),
        Message::Close(frame) => Some(tungstenite::Message::Close(frame.map(|f| {
            tungstenite::protocol::CloseFrame {
                code: tungstenite::protocol::frame::coding::CloseCode::from(f.code),
                reason: f.reason,
            }
        }))),
    }
}

fn tungstenite_to_axum(message: tungstenite::Message) -> Option<Message> {
    match message {
        tungstenite::Message::Text(text) => Some(Message::Text(text)),
        tungstenite::Message::Binary(bytes) => Some(Message::Binary(bytes.into())),
        tungstenite::Message::Ping(bytes) => Some(Message::Ping(bytes.into())),
        tungstenite::Message::Pong(bytes) => Some(Message::Pong(bytes.into())),
        tungstenite::Message::Close(frame) => Some(Message::Close(frame.map(|f| CloseFrame {
            code: f.code.into(),
            reason: f.reason,
        }))),
        tungstenite::Message::Frame(_) => None,
    }
}
