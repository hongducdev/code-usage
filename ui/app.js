const invoke = window.__TAURI__.core.invoke;
const win = window.__TAURI__.window.getCurrentWindow();
let dashboard = { providers: [], refreshing: false, lastRefresh: null };
let filter = "all";

const $ = (id) => document.getElementById(id);
const escapeHtml = (value = "") => String(value).replace(/[&<>'"]/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[char]);
const isReady = (provider) => ["ready", "detected", "refreshing"].includes(provider.status);
const formatPlanName = (value) => String(value || "").trim().replaceAll("_", " ").replace(/\b\p{L}/gu, (letter) => letter.toLocaleUpperCase("vi-VN"));

function statusText(provider) {
  return ({
    ready: provider.plan ? formatPlanName(provider.plan) : "Đã kết nối",
    detected: provider.plan ? formatPlanName(provider.plan) : "Đã phát hiện dữ liệu local",
    refreshing: "Đang làm mới…",
    needs_login: "Cần đăng nhập",
    needs_api_key: "Cần API key",
    error: provider.message || "Có lỗi",
  })[provider.status] || provider.status;
}

function resetIn(value) {
  if (!value) return "Không có lịch reset";
  const seconds = Math.max(0, Math.floor((new Date(value).getTime() - Date.now()) / 1000));
  if (seconds <= 0) return "Đang reset";
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days) return `Reset sau ${days} ngày ${hours} giờ`;
  if (hours) return `Reset sau ${hours} giờ ${minutes} phút`;
  return `Reset sau ${Math.max(1, minutes)} phút`;
}

function compactNumber(value) {
  return new Intl.NumberFormat("vi-VN", { notation: "compact", maximumFractionDigits: 1 }).format(value || 0);
}

function metricHtml(metric, provider) {
  const hasLimit = Number.isFinite(metric.limit) && metric.limit > 0;
  const used = Number(metric.used || 0);
  if (metric.unit === "%" && hasLimit) {
    const left = Math.min(100, Math.max(0, 100 - used));
    const usedPercent = Math.min(100, Math.max(0, used));
    return `<div class="quota">
      <div class="quota-title">${escapeHtml(metric.label)}</div>
      <div class="bar quota-bar" role="progressbar" aria-label="${escapeHtml(metric.label)} đã sử dụng" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${Math.round(usedPercent)}"><div class="fill" style="width:${usedPercent}%;--provider-color:${provider.color}"></div></div>
      <div class="quota-meta"><strong>${Math.round(usedPercent)}% đã dùng</strong><span>${Math.round(left)}% còn lại</span></div>
      ${metric.resetAt ? `<div class="quota-reset">${resetIn(metric.resetAt)}</div>` : ""}
    </div>`;
  }
  const value = metric.unit === "$" ? `$${used.toFixed(2)}` : `${compactNumber(used)} ${escapeHtml(metric.unit)}`;
  return `<div class="value-row"><span>${escapeHtml(metric.label)}</span><strong>${value}</strong>${metric.resetAt ? `<small>${resetIn(metric.resetAt)}</small>` : ""}</div>`;
}

function localUsageHtml(local) {
  const usage = local?.usage;
  if (!usage) return "";
  const points = usage.daily.slice(-14);
  const maximum = Math.max(1, ...points.map((point) => point.tokens));
  const bars = points.map((point) => {
    const height = point.tokens ? Math.max(6, Math.round(point.tokens / maximum * 100)) : 2;
    return `<i style="height:${height}%" title="${point.date}: ${compactNumber(point.tokens)} tokens · $${point.cost.toFixed(2)}"></i>`;
  }).join("");
  return `<section class="local-usage">
    <div class="stats-grid">
      <div><span>Hôm nay</span><strong>$${usage.todayCost.toFixed(2)}</strong></div>
      <div><span>Chi phí 30 ngày</span><strong>$${usage.cost30d.toFixed(2)}</strong></div>
      <div><span>Token 30 ngày</span><strong>${compactNumber(usage.tokens30d)}</strong></div>
      <div><span>Token gần nhất</span><strong>${compactNumber(usage.latestTokens)}</strong></div>
    </div>
    <div class="usage-chart" aria-label="Token usage 14 ngày">${bars}</div>
    <div class="model-note"><span>Model dùng nhiều nhất</span><strong>${escapeHtml(usage.topModel || "Không rõ")}</strong></div>
    <p class="estimate-note">${usage.estimatedCost ? "Chi phí ước tính từ token trong log local." : "Dữ liệu chi phí từ provider."}</p>
  </section>`;
}

function providerHtml(provider) {
  const metrics = provider.metrics.map((metric) => metricHtml(metric, provider)).join("");
  const local = provider.local?.detected
    ? `<div class="local-presence"><span>●</span><strong>${provider.local.sessionCount}</strong> file local${provider.local.lastActivity ? ` · ${relativeTime(provider.local.lastActivity)}` : ""}<small>${escapeHtml(provider.local.source || "")}</small></div>`
    : "";
  return `<article class="provider" data-state="${isReady(provider) ? "ready" : "attention"}">
    <div class="provider-head">
      <div class="provider-icon real-logo" style="--icon-bg:${provider.color}"><img src="logos/${provider.id}.svg" alt="${escapeHtml(provider.name)} logo"></div>
      <div class="provider-title"><h3>${escapeHtml(provider.name)}${provider.experimental ? '<span class="badge">experimental</span>' : ""}</h3><p>${escapeHtml(statusText(provider))}</p></div>
      <span class="state ${provider.status === "ready" ? "ready" : provider.status === "error" ? "error" : ""}"></span>
      <button class="provider-refresh" data-refresh="${provider.id}" title="Làm mới" aria-label="Làm mới ${escapeHtml(provider.name)}">↻</button>
    </div>
    ${local}
    ${metrics ? `<div class="metrics detail-metrics">${metrics}</div>` : `<div class="empty">${escapeHtml(provider.message || "Chưa có dữ liệu quota.")}</div>`}
    ${localUsageHtml(provider.local)}
  </article>`;
}

function relativeTime(value) {
  const seconds = Math.max(0, (Date.now() - new Date(value).getTime()) / 1000);
  if (seconds < 60) return "vừa xong";
  if (seconds < 3600) return `${Math.floor(seconds / 60)} phút trước`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)} giờ trước`;
  return `${Math.floor(seconds / 86400)} ngày trước`;
}

function render() {
  const providers = dashboard.providers.filter((provider) => filter === "all" || (filter === "ready" ? isReady(provider) : !isReady(provider)));
  $("providerList").innerHTML = providers.length ? providers.map(providerHtml).join("") : `<div class="empty empty-state"><strong>Không có nhà cung cấp phù hợp</strong><span>Chọn một bộ lọc khác để xem dữ liệu.</span></div>`;
  $("readyCount").textContent = dashboard.providers.filter(isReady).length;
  $("attentionCount").textContent = dashboard.providers.filter((provider) => !isReady(provider)).length;
  $("lastRefresh").textContent = dashboard.lastRefresh ? new Date(dashboard.lastRefresh).toLocaleTimeString("vi-VN", { hour: "2-digit", minute: "2-digit" }) : "Chưa có";
  $("refreshButton").classList.toggle("loading", dashboard.refreshing);
  $("providerList").classList.remove("loading-state");
  $("providerList").setAttribute("aria-busy", "false");
  document.querySelectorAll("[data-refresh]").forEach((button) => button.onclick = () => refreshProvider(button.dataset.refresh));
}

async function refreshStartupProviders() {
  const startupIds = new Set(["cursor", "copilot", "devin"]);
  const providers = dashboard.providers.filter((provider) => startupIds.has(provider.id) && isReady(provider));
  if (!providers.length) return;

  const previousStatuses = new Map(providers.map((provider) => [provider.id, provider.status]));
  dashboard.providers.forEach((provider) => {
    if (startupIds.has(provider.id) && isReady(provider)) provider.status = "refreshing";
  });
  render();

  const results = await Promise.allSettled(
    providers.map((provider) => invoke("refresh_provider", { id: provider.id }))
  );
  dashboard = await invoke("get_dashboard");
  render();

  const rejected = results.filter((result) => result.status === "rejected");
  if (rejected.length) {
    const failedNames = providers
      .filter((_, index) => results[index].status === "rejected")
      .map((provider) => provider.name)
      .join(", ");
    toast(`Không thể tự làm mới: ${failedNames}`);
  }
}

async function load() {
  try {
    dashboard = await invoke("get_dashboard");
    render();
    await refreshStartupProviders();
  } catch (error) {
    $("providerList").classList.remove("loading-state");
    $("providerList").setAttribute("aria-busy", "false");
    $("providerList").innerHTML = `<div class="empty error-state"><strong>Không thể tải dữ liệu</strong><span>${escapeHtml(String(error))}</span><button id="retryLoad">Thử lại</button></div>`;
    $("retryLoad").onclick = load;
    toast(String(error));
  }
}
async function refreshAll() { if (dashboard.refreshing) return; dashboard.refreshing = true; render(); try { dashboard = await invoke("refresh_all"); } catch (error) { toast(String(error)); } finally { dashboard.refreshing = false; render(); } }
async function refreshProvider(id) { try { dashboard = await invoke("refresh_provider", { id }); render(); } catch (error) { toast(String(error)); } }
function toast(message) { const element = $("toast"); element.textContent = message; element.classList.add("show"); setTimeout(() => element.classList.remove("show"), 2600); }

window.refreshAll = refreshAll;
$("refreshButton").onclick = refreshAll;
$("petToggleButton").onclick = () => invoke("toggle_pet_visibility");
$("hideButton").onclick = () => win.hide();
$("settingsButton").onclick = () => $("settingsDialog").showModal();
$("closeSettings").onclick = () => $("settingsDialog").close();
document.querySelectorAll("nav button").forEach((button) => button.onclick = () => { document.querySelector("nav .active").classList.remove("active"); button.classList.add("active"); filter = button.dataset.filter; render(); });
$("saveKeys").onclick = async () => { try { for (const [provider, input] of [["openrouter", $("openrouterKey")], ["zai", $("zaiKey")]]) if (input.value.trim()) dashboard = await invoke("save_api_key", { provider, value: input.value.trim() }); $("settingsDialog").close(); render(); toast("Đã lưu API key an toàn"); } catch (error) { toast(String(error)); } };
$("clearKeys").onclick = async () => { try { for (const provider of ["openrouter", "zai"]) dashboard = await invoke("save_api_key", { provider, value: "" }); render(); toast("Đã xóa API key"); } catch (error) { toast(String(error)); } };
load();
window.setInterval(refreshAll, 60_000);
