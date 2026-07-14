const invoke = window.__TAURI__.core.invoke;
const currentWindow = window.__TAURI__.window.getCurrentWindow();
const app = document.getElementById("petApp");
const list = document.getElementById("agentList");
let agents = [];
let expanded = false;
let filter = "all";
let petPreference = localStorage.getItem("agent-pet-style") || "auto";
let dragCandidate = null;
let dragging = false;

const claudeSvg = `<svg viewBox="0 0 18 12" shape-rendering="crispEdges" fill="currentColor" aria-hidden="true"><rect x="3" y="0" width="12" height="2.4"></rect><rect x="3" y="2.4" width="2" height="2.4"></rect><rect x="6" y="2.4" width="6" height="2.4"></rect><rect x="13" y="2.4" width="2" height="2.4"></rect><rect x="1" y="4.8" width="16" height="2.4"></rect><rect x="3" y="7.199999999999999" width="12" height="2.4"></rect><rect x="4" y="9.6" width="1" height="2.4"></rect><rect x="6" y="9.6" width="1" height="2.4"></rect><rect x="11" y="9.6" width="1" height="2.4"></rect><rect x="13" y="9.6" width="1" height="2.4"></rect></svg>`;

const escapeHtml = (value = "") => String(value).replace(/[&<>'"]/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[char]);
const isActive = (agent) => ["working", "needs_approval", "waiting"].includes(agent.status);
const statusText = (status) => ({ working: "Đang chạy", needs_approval: "Chờ phê duyệt", waiting: "Đang chờ", completed: "Hoàn tất", error: "Có lỗi" })[status] || status;

function relativeTime(value) {
  const seconds = Math.max(0, Math.floor((Date.now() - new Date(value).getTime()) / 1000));
  if (seconds < 10) return "vừa cập nhật";
  if (seconds < 60) return `${seconds} giây trước`;
  return `${Math.floor(seconds / 60)} phút trước`;
}

function agentHtml(agent) {
  const shortId = agent.id.slice(0, 6);
  return `<article class="agent" data-status="${escapeHtml(agent.status)}">
    <div class="agent-logo"><img src="logos/${escapeHtml(agent.provider)}.svg" alt="${escapeHtml(agent.provider)}"></div>
    <div class="agent-copy">
      <div class="agent-name"><strong>${escapeHtml(agent.workspace)}</strong><span class="status ${escapeHtml(agent.status)}">${statusText(agent.status)}</span></div>
      <p class="agent-message">${escapeHtml(agent.message)}</p>
      <div class="agent-meta"><span>${escapeHtml(agent.provider)}</span><span>#${escapeHtml(shortId)}</span></div>
    </div>
    <time class="agent-time" datetime="${escapeHtml(agent.updatedAt)}">${relativeTime(agent.updatedAt)}</time>
    <div class="agent-progress" role="progressbar" aria-label="Tiến trình ${escapeHtml(agent.workspace)}" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${agent.progress}"><i style="width:${agent.progress}%"></i></div>
  </article>`;
}

function render() {
  const active = agents.filter(isActive);
  const approval = active.filter((agent) => agent.status === "needs_approval");
  const visible = agents.filter((agent) => filter === "all" || (filter === "active" ? isActive(agent) : !isActive(agent)));
  const automaticProvider = (approval[0] || active[0])?.provider === "claude" ? "claude" : "codex";
  const petProvider = petPreference === "auto" ? automaticProvider : petPreference;
  app.dataset.provider = petProvider;
  app.dataset.mode = approval.length ? "approval" : active.length ? "working" : "idle";
  document.getElementById("agentCount").textContent = active.length;
  document.getElementById("agentCount").setAttribute("aria-label", `${active.length} agent đang chạy`);
  document.getElementById("activeCount").textContent = active.length;
  document.getElementById("recentCount").textContent = Math.max(0, agents.length - active.length);
  document.getElementById("compactLabel").textContent = approval.length ? "Đang chờ bạn duyệt" : active.length ? `${active.length} agent đang chạy` : "Đang nghỉ";
  document.getElementById("panelTitle").textContent = approval.length ? `${approval.length} agent cần phê duyệt` : active.length ? `${active.length} agent đang làm việc` : "Không có agent đang chạy";
  const miniPet = document.getElementById("miniPet");
  miniPet.classList.toggle("claude", petProvider === "claude");
  miniPet.innerHTML = petProvider === "claude" ? claudeSvg : "<i></i><i></i><b></b>";
  document.getElementById("petStyleButton").textContent = `Pet: ${{ auto: "tự động", codex: "Codex", claude: "Claude" }[petPreference]}`;
  document.getElementById("scanStatus").textContent = `Cập nhật ${new Date().toLocaleTimeString("vi-VN", { hour: "2-digit", minute: "2-digit", second: "2-digit" })}`;
  list.setAttribute("aria-busy", "false");
  list.innerHTML = visible.length ? visible.map(agentHtml).join("") : `<div class="agent-empty"><div class="sleep-face" aria-hidden="true"></div><strong>Chưa có hoạt động</strong><span>Pet sẽ tự hiển thị khi Codex hoặc Claude cập nhật phiên cục bộ.</span></div>`;
}

async function refresh() {
  try {
    agents = await invoke("get_agent_activity");
    render();
  } catch (error) {
    list.setAttribute("aria-busy", "false");
    list.innerHTML = `<div class="agent-empty"><strong>Không thể quét agent</strong><span>${escapeHtml(String(error))}</span></div>`;
  }
}

async function setExpanded(next) {
  if (next === expanded) return;
  await invoke("set_pet_expanded", { expanded: next });
  expanded = next;
  app.dataset.state = expanded ? "expanded" : "collapsed";
  document.getElementById("petButton").setAttribute("aria-expanded", String(expanded));
  if (expanded) refresh();
}

const petButton = document.getElementById("petButton");
petButton.onpointerdown = (event) => {
  if (event.button !== 0) return;
  petButton.setPointerCapture(event.pointerId);
  dragCandidate = { x: event.screenX, y: event.screenY, pointerId: event.pointerId };
};
window.addEventListener("pointermove", async (event) => {
  if (!dragCandidate || dragging || event.pointerId !== dragCandidate.pointerId) return;
  if (Math.hypot(event.screenX - dragCandidate.x, event.screenY - dragCandidate.y) < 5) return;
  dragging = true;
  dragCandidate = null;
  app.dataset.dragging = "true";
  try {
    await currentWindow.startDragging();
  } finally {
    app.dataset.dragging = "false";
    dragging = false;
    if (petButton.hasPointerCapture(event.pointerId)) petButton.releasePointerCapture(event.pointerId);
  }
});
window.addEventListener("pointerup", (event) => {
  if (!dragCandidate || event.pointerId !== dragCandidate.pointerId) return;
  dragCandidate = null;
  if (petButton.hasPointerCapture(event.pointerId)) petButton.releasePointerCapture(event.pointerId);
  if (!dragging) setExpanded(true);
});
window.addEventListener("pointercancel", (event) => {
  dragCandidate = null;
  if (petButton.hasPointerCapture(event.pointerId)) petButton.releasePointerCapture(event.pointerId);
});
document.getElementById("collapseButton").onclick = () => setExpanded(false);
document.getElementById("hidePet").onclick = () => currentWindow.hide();
document.getElementById("openMain").onclick = () => invoke("show_main_window");
document.getElementById("refreshAgents").onclick = refresh;
document.getElementById("petStyleButton").onclick = () => {
  petPreference = ({ auto: "codex", codex: "claude", claude: "auto" })[petPreference];
  localStorage.setItem("agent-pet-style", petPreference);
  render();
};
document.querySelectorAll("nav button").forEach((button) => button.onclick = () => {
  document.querySelector("nav button.active")?.classList.remove("active");
  button.classList.add("active");
  filter = button.dataset.filter;
  render();
});

refresh();
window.setInterval(refresh, 2_500);
