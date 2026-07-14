const invoke = window.__TAURI__.core.invoke;
const currentWindow = window.__TAURI__.window.getCurrentWindow();
const app = document.getElementById("petApp");
const bubble = document.getElementById("activityBubble");
const petButton = document.getElementById("petButton");

const BUBBLE_TRANSITION_MS = 240;
const priority = { needs_approval: 0, error: 1, completed: 2, waiting: 3, working: 4 };
const labels = {
  working: "Đang chạy",
  needs_approval: "Cần bạn xử lý",
  waiting: "Đang chờ",
  completed: "Sẵn sàng",
  error: "Bị gián đoạn",
};

let agents = [];
let expanded = false;
let initialized = false;
let selectedAgentId = null;
let previousFingerprints = new Map();
let hideTimer = null;
let dragCandidate = null;
let dragging = false;
let transitionVersion = 0;
let autoHideMs = 7_500;
let petSettings = { animationsEnabled: true, privacyMode: false, petScale: 1 };

const isActive = (agent) => ["working", "needs_approval", "waiting"].includes(agent.status);
const fingerprint = (agent) => agent.status === "working"
  ? agent.status
  : `${agent.status}|${agent.message}`;

function rankedAgents() {
  return [...agents].sort((a, b) => {
    const priorityDifference = (priority[a.status] ?? 9) - (priority[b.status] ?? 9);
    return priorityDifference || new Date(b.updatedAt) - new Date(a.updatedAt);
  });
}

function selectedAgent() {
  return agents.find((agent) => agent.id === selectedAgentId) || rankedAgents()[0] || null;
}

function render() {
  const active = agents.filter(isActive);
  const approval = active.filter((agent) => agent.status === "needs_approval");
  const featured = selectedAgent();

  app.dataset.provider = "claude";
  app.dataset.mode = approval.length
    ? "approval"
    : active.some((agent) => agent.status === "working")
      ? "working"
      : active.some((agent) => agent.status === "waiting")
        ? "waiting"
        : featured?.status === "error"
          ? "error"
          : featured?.status === "completed"
            ? "ready"
            : "idle";
  document.getElementById("agentCount").textContent = active.length;
  document.getElementById("agentCount").hidden = active.length === 0;
  document.getElementById("agentCount").setAttribute("aria-label", `${active.length} agent đang chạy`);
  if (!featured) {
    document.getElementById("bubbleWorkspace").textContent = "Chưa có hoạt động";
    document.getElementById("bubbleStatus").textContent = "Đang nghỉ";
    document.getElementById("bubbleMessage").textContent = "Pet sẽ báo khi agent có cập nhật mới.";
    document.getElementById("bubbleProgress").style.transform = "scaleX(0)";
    return;
  }

  bubble.dataset.status = featured.status;
  document.getElementById("bubbleLogo").src = `logos/${featured.provider}.svg`;
  document.getElementById("bubbleWorkspace").textContent = petSettings.privacyMode ? "Agent" : featured.workspace;
  document.getElementById("bubbleStatus").textContent = labels[featured.status] || featured.status;
  document.getElementById("bubbleMessage").textContent = petSettings.privacyMode
    ? (featured.status === "needs_approval" ? "Có tác vụ cần bạn xử lý." : "Agent vừa cập nhật trạng thái.")
    : featured.message;
  document.getElementById("bubbleProgress").style.transform = `scaleX(${Math.max(0, Math.min(100, featured.progress)) / 100})`;
}

function scheduleHide() {
  clearTimeout(hideTimer);
  hideTimer = window.setTimeout(() => setExpanded(false), autoHideMs);
}

async function applySettings() {
  try {
    petSettings = await invoke("get_app_settings");
    autoHideMs = petSettings.petAutoHideMs;
    app.dataset.animations = petSettings.animationsEnabled ? "on" : "off";
    app.style.setProperty("--pet-scale", String(petSettings.petScale));
    render();
    if (expanded) scheduleHide();
  } catch (error) {
    console.error("Không thể tải cài đặt pet", error);
  }
}

window.applySettings = applySettings;

async function setExpanded(next, agent = null) {
  if (agent) selectedAgentId = agent.id;
  if (next === expanded) {
    if (next) scheduleHide();
    render();
    return;
  }

  const version = ++transitionVersion;
  try {
    if (next) {
      await invoke("set_pet_expanded", { expanded: true });
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      if (version !== transitionVersion) return;
      expanded = true;
      app.dataset.state = "expanded";
      bubble.setAttribute("aria-hidden", "false");
      petButton.setAttribute("aria-expanded", "true");
      scheduleHide();
    } else {
      expanded = false;
      app.dataset.state = "collapsed";
      bubble.setAttribute("aria-hidden", "true");
      petButton.setAttribute("aria-expanded", "false");
      clearTimeout(hideTimer);
      await new Promise((resolve) => window.setTimeout(resolve, BUBBLE_TRANSITION_MS));
      if (version !== transitionVersion) return;
      await invoke("set_pet_expanded", { expanded: false });
    }
    render();
  } catch (error) {
    console.error("Không thể đổi kích thước pet", error);
  }
}

async function refresh() {
  try {
    const nextAgents = await invoke("get_agent_activity");
    const changed = nextAgents
      .filter((agent) => previousFingerprints.get(agent.id) !== fingerprint(agent))
      .sort((a, b) => (priority[a.status] ?? 9) - (priority[b.status] ?? 9));

    agents = nextAgents;
    render();

    if (initialized && changed.length) {
      await setExpanded(true, changed[0]);
      const notable = changed.filter((agent) => ["needs_approval", "completed", "error"].includes(agent.status));
      Promise.allSettled(notable.map((agent) => invoke("notify_agent_event", {
        workspace: agent.workspace,
        status: agent.status,
        message: agent.message,
      })));
    }

    previousFingerprints = new Map(nextAgents.map((agent) => [agent.id, fingerprint(agent)]));
    initialized = true;
  } catch (error) {
    console.error("Không thể quét hoạt động agent", error);
  }
}

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
  if (!dragging) setExpanded(!expanded);
});

window.addEventListener("pointercancel", () => {
  dragCandidate = null;
});

document.getElementById("dismissBubble").onclick = (event) => {
  event.stopPropagation();
  setExpanded(false);
};
document.getElementById("bubbleButton").onclick = () => invoke("show_main_window");
bubble.addEventListener("pointerenter", () => clearTimeout(hideTimer));
bubble.addEventListener("pointerleave", scheduleHide);
window.addEventListener("keydown", (event) => {
  if (event.key === "Escape") setExpanded(false);
});

applySettings().then(refresh);
window.setInterval(refresh, 2_500);
