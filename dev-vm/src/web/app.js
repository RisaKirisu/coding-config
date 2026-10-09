import * as textStore from './text-store.js';

const $ = id => document.getElementById(id);
const app = $('app');
const wideLayout = matchMedia('(min-width: 960px), (horizontal-viewport-segments: 2)');
const frames = new Map();
const projectRows = new Map();
const commands = new Map();
const pendingClaims = new Map();
const projectSessions = new Map();
let projects = [];
let daemonInstanceId = null;
let selectedProject = null;
let selectedSession = null;
let projectsRequest = null;
let frameReconciliation = null;
let switchRevision = 0;
let logsRequest = null;
let logsProject = null;
let switching = false;
let hasSelectionNotice = false;
let placeholderAction = () => setProjectsOpen(true);

// Only the outer document sees the keyboard's visual viewport; the iframe fills it.
function updateViewport() {
  const viewport = window.visualViewport;
  if (!viewport || viewport.scale !== 1) return;
  app.style.setProperty('--app-viewport-height', `${viewport.height}px`);
  app.style.setProperty('--app-viewport-top', `${viewport.offsetTop}px`);
}
window.visualViewport?.addEventListener('resize', updateViewport);
window.visualViewport?.addEventListener('scroll', updateViewport);
updateViewport();

function notify(message, isError = false) {
  hasSelectionNotice = false;
  $('notice-text').textContent = message;
  $('notice').dataset.error = String(isError);
  $('notice').hidden = false;
}

function clearSelectionNotice() {
  if (!hasSelectionNotice) return;
  $('notice').hidden = true;
  hasSelectionNotice = false;
}

function setProjectsOpen(isOpen) {
  app.dataset.projectsOpen = String(isOpen);
  $('projects-panel').inert = !isOpen;
  $('open-projects').setAttribute('aria-expanded', String(isOpen));
  $('projects-backdrop').hidden = !isOpen || wideLayout.matches;
  if (isOpen) {
    const frame = frames.get(selectedProject);
    if (frame?.ready) post(frame, 'close-navigation');
  }
}

function renderDaemon(connected) {
  $('daemon-status').dataset.state = connected ? 'connected' : 'unavailable';
  $('daemon-status').lastElementChild.textContent = connected ? 'Control Daemon connected' : 'Control Daemon unavailable';
}

async function api(url, options = {}) {
  const response = await fetch(url, {cache: 'no-store', ...options});
  const value = await response.json();
  if (!response.ok) throw new Error(value.error || `Request failed (${response.status}).`);
  return value;
}

function launchUrl(project) {
  const remote = !location.hostname.endsWith('.localhost') && location.hostname !== 'localhost';
  const value = remote ? project.links.tailnet_dsh_url : project.links.local_dsh_url;
  if (!value) return null;
  const url = new URL(value);
  const validLocal = location.hostname === 'control.devvm.localhost' && url.hostname === `3080.${project.project_host}.devvm.localhost` && url.protocol === 'http:';
  const validRemote = location.hostname.startsWith('devvm.') && url.hostname === `${project.project_host}-3080.${location.hostname.slice(6)}` && url.protocol === 'https:';
  if (!validLocal && !validRemote) throw new Error('Project launch address does not match this Control address.');
  return url;
}

function projectSummary(project) {
  if (project.operation) return `${project.operation.action} · ${project.operation.state.replaceAll('_', ' ')}`;
  if (project.dsh_status === 'running') return 'DSH running';
  if (project.dsh_status === 'starting') return 'DSH starting';
  if (project.vm_status === 'running') return 'VM running · DSH stopped';
  return `VM ${project.vm_status}`;
}

function renderProjects() {
  const list = $('project-list');
  const filter = $('project-search').value.toLocaleLowerCase();
  const retained = new Set();
  for (const project of projects) {
    retained.add(project.id);
    let row = projectRows.get(project.id);
    if (!row) {
      row = document.createElement('button');
      row.className = 'project-row';
      row.dataset.projectId = project.id;
      const avatar = document.createElement('span');
      avatar.className = 'project-avatar';
      avatar.setAttribute('aria-hidden', 'true');
      const text = document.createElement('span');
      text.className = 'project-row-text';
      const name = document.createElement('span');
      name.className = 'project-row-name';
      const status = document.createElement('span');
      status.className = 'project-row-status';
      const dot = document.createElement('span');
      dot.className = 'status-dot';
      dot.setAttribute('aria-hidden', 'true');
      const label = document.createElement('span');
      status.append(dot, label);
      const memory = document.createElement('span');
      memory.className = 'project-row-memory';
      text.append(name, status, memory);
      row.append(avatar, text);
      row.addEventListener('click', () => navigate(project.id));
      projectRows.set(project.id, row);
    }
    const summary = projectSummary(project);
    const text = row.lastElementChild;
    row.firstElementChild.textContent = project.name.slice(0, 1).toLocaleUpperCase();
    text.firstElementChild.textContent = project.name;
    const statusEl = text.children[1];
    statusEl.lastElementChild.textContent = summary;
    statusEl.firstElementChild.dataset.status = project.operation ? 'pending' : project.dsh_status;
    const memoryEl = text.children[2];
    const memoryText = (project.vm_status === 'running' && project.memory?.formatted) ? project.memory.formatted : '';
    if (memoryEl) {
      memoryEl.textContent = memoryText;
      memoryEl.hidden = !memoryText;
    }
    row.setAttribute('aria-label', `${project.name}, ${summary}${memoryText ? `, memory ${memoryText}` : ''}`);
    row.setAttribute('aria-current', project.id === selectedProject ? 'page' : 'false');
    row.hidden = !`${project.name} ${project.path}`.toLocaleLowerCase().includes(filter);
    if (row.parentElement !== list) list.append(row);
  }
  for (const [id, row] of projectRows) {
    if (!retained.has(id)) { row.remove(); projectRows.delete(id); }
  }
  list.querySelector('.list-empty')?.remove();
  if (![...projectRows.values()].some(row => !row.hidden)) {
    const empty = document.createElement('p');
    empty.className = 'list-empty';
    empty.textContent = projects.length ? 'No matching Projects.' : 'No Projects yet. Add one below.';
    list.append(empty);
  }
  renderSelectedProject();
}

async function refreshProjects() {
  if (projectsRequest) return projectsRequest;
  projectsRequest = (async () => {
    try {
      const refreshed = await api('/api/projects');
      const instanceId = refreshed[0]?.daemon_instance_id;
      if (daemonInstanceId && instanceId && daemonInstanceId !== instanceId) notify('Control Daemon restarted. Earlier operation progress is unavailable; current VM and DSH status is shown.');
      if (instanceId) daemonInstanceId = instanceId;
      projects = refreshed;
      renderDaemon(true);
      renderProjects();
      await reconcileFrames();
      if (selectedProject && !frames.has(selectedProject)) await openSelectedFrame();
      return refreshed;
    } catch (error) {
      renderDaemon(false);
      if (!projects.length) $('project-list').querySelector('.list-empty').textContent = 'Cannot reach the daemon. Reconnecting…';
      if (selectedProject && !frames.has(selectedProject)) showPlaceholder('Control Daemon unavailable', 'Your text is saved on this device. The Project will become available when its status can be read.', 'Try again', refreshProjects);
    } finally { projectsRequest = null; }
  })();
  return projectsRequest;
}

function renderSelectedProject() {
  const project = projects.find(item => item.id === selectedProject);
  $('project-name').textContent = project?.name || (selectedProject ? 'Project unavailable' : 'Your workspace');
  $('project-actions').hidden = !project;
  $('project-title').setAttribute('aria-label', project ? `${project.name}: choose Project` : 'Choose Project');
  if ($('actions-dialog').open && project) renderActions(project);
}

function showPlaceholder(title, description, actionLabel, action) {
  $('workspace-placeholder').hidden = false;
  $('placeholder-title').textContent = title;
  $('placeholder-description').textContent = description;
  $('placeholder-action').textContent = actionLabel || '';
  $('placeholder-action').hidden = !actionLabel;
  placeholderAction = action;
  const project = projects.find(item => item.id === selectedProject);
  $('placeholder-progress').hidden = !project?.operation;
  $('placeholder-progress').textContent = project?.operation ? projectSummary(project) : '';
}

async function reauthorize(frame, force = false) {
  const project = projects.find(item => item.id === frame.projectId);
  if (!project || project.dsh_status !== 'running' || !frame.ready || frame.authorizing) return;
  const target = launchUrl(project);
  if (!target) throw new Error('Project authorization is not available yet.');
  const url = target.href;
  if (!force && frame.authorizationUrl === url) return;
  frame.authorizationUrl = url;
  frame.authorizing = true;
  try { await requestChild(frame, 'reauthorize', {url}); }
  catch (error) { notify(error.message, true); }
  finally { frame.authorizing = false; }
}

function renderConnection(frame) {
  if (frame.projectId !== selectedProject) return;
  const project = projects.find(item => item.id === frame.projectId);
  const status = project && !['running', 'starting'].includes(project.dsh_status) ? 'stopped' : frame.connection || 'connecting';
  const labels = {connecting: 'Connecting', recovering: 'Reconnecting', disconnected: 'Waiting for connection', 'auth-required': 'Sign in required', connected: '', stopped: 'DSH stopped'};
  $('connection-status').textContent = labels[status] || status;
  $('connection-status').dataset.state = status;
  $('connection-status').hidden = status === 'connected';
  $('connection-status').title = status === 'auth-required' ? 'Authorize this Project again' : 'Reconnect to this Project';
  if (frame.ready) $('workspace-placeholder').hidden = true;
}

function post(frame, kind, payload = {}, commandId) {
  frame.element.contentWindow.postMessage({protocol: 'devvm-embed', version: 1, projectId: frame.projectId, channelId: frame.channelId, instanceId: frame.instanceId, kind, payload, commandId}, frame.origin);
}

function requestChild(frame, kind, payload = {}) {
  const commandId = crypto.randomUUID();
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => { commands.delete(commandId); reject(new Error('DSH did not finish saving this view. Please try again.')); }, 15000);
    commands.set(commandId, {frame, resolve, reject, timeout});
    post(frame, kind, payload, commandId);
  });
}

async function selectFrameSession(frame, sessionId) {
  const selection = {sessionId, revision: switchRevision, instanceId: frame.instanceId};
  frame.selectingSession = selection;
  const isCurrentSelection = () => frame.selectingSession === selection
    && frames.get(frame.projectId) === frame
    && frame.instanceId === selection.instanceId
    && switchRevision === selection.revision
    && selectedProject === frame.projectId && selectedSession === sessionId;
  try {
    await requestChild(frame, 'select-session', {sessionId});
    if (isCurrentSelection()) clearSelectionNotice();
  } catch (error) {
    if (isCurrentSelection()) {
      notify(error.message, true);
      hasSelectionNotice = true;
    }
  } finally {
    if (frame.selectingSession === selection) frame.selectingSession = null;
  }
}

function createFrame(project, url) {
  const element = document.createElement('iframe');
  element.className = 'project-frame';
  element.title = `DSH — ${project.name}`;
  element.referrerPolicy = 'no-referrer';
  element.allow = 'microphone; clipboard-read; clipboard-write; fullscreen';
  const frame = {element, projectId: project.id, origin: url.origin, channelId: crypto.randomUUID(), instanceId: null, ready: false, connection: 'connecting', sessionId: null, selectingSession: null};
  frames.set(project.id, frame);
  element.src = url.href; // Launch credentials are used transiently, never written to storage.
  $('frames').append(element);
  return frame;
}

function releaseFrame(frame) {
  if (frames.get(frame.projectId) !== frame) return;
  for (const [id, command] of commands) {
    if (command.frame === frame) {
      clearTimeout(command.timeout);
      command.reject(new Error('The Project view changed.'));
      commands.delete(id);
    }
  }
  for (const [id, owner] of pendingClaims) if (owner === frame) pendingClaims.delete(id);
  frames.delete(frame.projectId);
  frame.element.remove();
}

function reconcileFrames() {
  if (frameReconciliation) return frameReconciliation;
  frameReconciliation = reconcileFrameState().finally(() => { frameReconciliation = null; });
  return frameReconciliation;
}

async function reconcileFrameState() {
  const pending = await textStore.readPending();
  $('open-saved-text').hidden = pending.length === 0;
  $('saved-text-count').textContent = pending.length;
  $('open-saved-text').setAttribute('aria-label', `Saved text, ${pending.length} pending`);
  const pendingProjects = new Set(pending.map(record => record.projectId));
  for (const frame of frames.values()) {
    const project = projects.find(item => item.id === frame.projectId);
    if (!project || frame.projectId !== selectedProject && (project.dsh_status === 'stopped' || project.vm_status === 'stopped')) {
      // Persist text before removing a frame whose Runtime stopped.
      if (frame.ready) await requestChild(frame, 'prepare-switch');
      if (frame.projectId === selectedProject && projects.some(item => item.id === frame.projectId)) continue;
      releaseFrame(frame);
      continue;
    }
    if (frame.projectId !== selectedProject && !pendingProjects.has(frame.projectId)) {
      if (frame.ready) await requestChild(frame, 'prepare-switch');
      releaseFrame(frame);
    }
  }
  for (const id of pendingProjects) {
    if (frames.has(id)) continue;
    const project = projects.find(item => item.id === id);
    if (project?.dsh_status !== 'running') continue;
    const url = launchUrl(project);
    if (url) createFrame(project, url);
  }
  for (const frame of frames.values()) {
    const project = projects.find(item => item.id === frame.projectId);
    if (frame.ready && project?.dsh_status === 'running') {
      if (frame.connection === 'auth-required') await reauthorize(frame);
      await deliverPending(frame, pending);
    }
    renderConnection(frame);
  }
}

async function deliverPending(frame, records) {
  if (frame.connection !== 'connected') return;
  const pending = records || await textStore.readPending();
  for (const record of pending) {
    if (record.projectId !== frame.projectId || record.state === 'blocked' || pendingClaims.has(record.requestId)) continue;
    pendingClaims.set(record.requestId, frame);
    post(frame, 'deliver', {sessionId: record.sessionId, requestId: record.requestId, text: record.text, mode: record.mode});
  }
}

async function openSelectedFrame() {
  const project = projects.find(item => item.id === selectedProject);
  if (!project) {
    if (selectedProject) showPlaceholder('Project unavailable', 'This Project is no longer registered. Saved text stays associated with its original Project.', 'Choose Project', () => setProjectsOpen(true));
    return;
  }
  let frame = frames.get(project.id);
  if (!frame) {
    if (project.dsh_status !== 'running') {
      const starting = project.dsh_status === 'starting' || project.operation;
      showPlaceholder(starting ? 'Getting your Project ready' : 'Ready when you are', starting ? 'The daemon is working. This view will open when DSH is ready.' : `Launch DSH in ${project.name} to open your conversations. Your saved text stays on this device.`, starting ? 'View progress' : 'Launch DSH', starting ? () => openActions() : () => submitAction(project.id, 'launch_dsh'));
      return;
    }
    const url = launchUrl(project);
    if (!url) { showPlaceholder('Launch address unavailable', 'The daemon has not captured the current DSH launch credentials yet.', 'Refresh status', refreshProjects); return; }
    frame = createFrame(project, url);
  }
  for (const item of frames.values()) item.element.dataset.active = String(item === frame);
  if (!frame.ready) showPlaceholder('Opening your conversation', 'Connecting to this Project’s DSH workspace.', null, null);
  renderConnection(frame);
  if (frame.ready && selectedSession && frame.sessionId !== selectedSession) {
    await selectFrameSession(frame, selectedSession);
  }
}

async function navigate(projectId, sessionId = projectSessions.get(projectId) || null, historyMode = 'push') {
  const revision = ++switchRevision;
  const oldFrame = frames.get(selectedProject);
  switching = true;
  try {
    if (oldFrame?.ready && (projectId !== selectedProject || sessionId !== selectedSession)) await requestChild(oldFrame, 'prepare-switch');
    if (revision !== switchRevision) return;
    selectedProject = projectId;
    selectedSession = sessionId;
    clearSelectionNotice();
    $('session-title').textContent = '';
    if (historyMode !== 'none') {
      const route = projectId ? `/#/projects/${encodeURIComponent(projectId)}${sessionId ? `/sessions/${encodeURIComponent(sessionId)}` : ''}` : '/';
      history[historyMode === 'replace' ? 'replaceState' : 'pushState'](null, '', route);
    }
    if (!wideLayout.matches) setProjectsOpen(false);
    for (const frame of frames.values()) frame.element.dataset.active = String(frame.projectId === projectId);
    renderProjects();
    await openSelectedFrame();
    await reconcileFrames();
  } catch (error) { if (revision === switchRevision) notify(error.message, true); }
  finally { if (revision === switchRevision) switching = false; }
}

function route() {
  const match = /^#\/projects\/([0-9a-f-]+)(?:\/sessions\/([^/]+))?$/.exec(location.hash);
  return match ? {project: match[1], session: match[2] ? decodeURIComponent(match[2]) : null} : {project: null, session: null};
}

async function receiveBridge(event) {
  const frame = [...frames.values()].find(item => event.source === item.element.contentWindow && event.origin === item.origin);
  const message = event.data;
  if (!frame || !message || message.protocol !== 'devvm-embed' || message.version !== 1 || message.projectId !== frame.projectId) return;
  if (message.kind === 'available') {
    if (typeof message.instanceId !== 'string') return;
    if (message.instanceId !== frame.expectedInstanceId) {
      frame.expectedInstanceId = message.instanceId;
      frame.channelId = crypto.randomUUID();
      frame.instanceId = null;
      frame.ready = false;
      frame.selectingSession = null;
      frame.connection = 'connecting';
      for (const [id, owner] of pendingClaims) if (owner === frame) pendingClaims.delete(id);
    }
    post(frame, 'attach');
    return;
  }
  if (message.channelId !== frame.channelId || typeof message.instanceId !== 'string') return;
  if (message.kind === 'ready') {
    if (message.instanceId !== frame.expectedInstanceId) return;
    frame.instanceId = message.instanceId;
    frame.ready = true;
    frame.connection = message.payload.connection;
    frame.sessionId = message.payload.sessionId;
    renderConnection(frame);
    if (frame.projectId === selectedProject && selectedSession && selectedSession !== frame.sessionId) {
      await selectFrameSession(frame, selectedSession);
    }
    await deliverPending(frame);
    return;
  }
  if (message.instanceId !== frame.instanceId) return;
  const {kind, payload, commandId} = message;
  if (!payload || typeof payload !== 'object') return;
  if (kind === 'result') {
    const command = commands.get(commandId);
    if (!command || command.frame !== frame) return;
    clearTimeout(command.timeout);
    commands.delete(commandId);
    if (payload.error) command.reject(new Error(payload.error));
    else command.resolve(payload);
    return;
  }
  if (kind === 'connection-changed') {
    frame.connection = payload.state;
    renderConnection(frame);
    if (payload.state === 'auth-required') await reauthorize(frame);
    if (payload.state !== 'connected') {
      for (const [id, owner] of pendingClaims) if (owner === frame) pendingClaims.delete(id);
    } else await deliverPending(frame);
    return;
  }
  if (kind === 'selection-changed') {
    if (frame.selectingSession && payload.sessionId !== frame.selectingSession.sessionId) return;
    frame.sessionId = payload.sessionId;
    if (payload.sessionId) projectSessions.set(frame.projectId, payload.sessionId);
    if (frame.projectId === selectedProject && !switching) {
      selectedSession = payload.sessionId;
      if (payload.sessionId) clearSelectionNotice();
      $('session-title').textContent = typeof payload.title === 'string' ? payload.title : '';
      history.replaceState(null, '', `/#/projects/${frame.projectId}${selectedSession ? `/sessions/${encodeURIComponent(selectedSession)}` : ''}`);
    }
    return;
  }
  if (kind === 'navigation-opened') { if (!wideLayout.matches) setProjectsOpen(false); return; }
  if (kind === 'error') { notify(payload.message, true); return; }
  const reply = (value = {}) => {
    if (frames.get(frame.projectId) === frame && message.channelId === frame.channelId) post(frame, 'result', value, commandId);
  };
  try {
    if (typeof payload.sessionId !== 'string') throw new Error('Invalid session identity.');
    if (kind === 'draft-load') reply(await textStore.readDraft(frame.projectId, payload.sessionId));
    else if (kind === 'draft-save') {
      if (typeof payload.text !== 'string' || !Number.isSafeInteger(payload.revision) || payload.revision < 0) throw new Error('Invalid draft.');
      reply(await textStore.writeDraft(frame.projectId, payload));
    } else if (kind === 'prompt-admit') {
      if (typeof payload.text !== 'string' || typeof payload.requestId !== 'string' || !['queue', 'steer'].includes(payload.mode) || !Number.isSafeInteger(payload.draftRevision)) throw new Error('Invalid pending text.');
      const receipt = await textStore.admitText(frame.projectId, payload);
      pendingClaims.set(payload.requestId, frame);
      reply(receipt);
    } else if (kind === 'prompt-settled') {
      if (pendingClaims.get(payload.requestId) !== frame) return;
      await textStore.settleText(frame.projectId, payload.sessionId, payload.requestId);
      pendingClaims.delete(payload.requestId);
      reply();
    } else if (kind === 'prompt-waiting') {
      if (pendingClaims.get(payload.requestId) === frame) pendingClaims.delete(payload.requestId);
      reply();
    } else if (kind === 'prompt-blocked') {
      if (pendingClaims.get(payload.requestId) !== frame || typeof payload.error !== 'string') return;
      await textStore.blockText(frame.projectId, payload.sessionId, payload.requestId, payload.error);
      pendingClaims.delete(payload.requestId);
      notify(`Pending text needs attention: ${payload.error}`, true);
      reply();
    }
  } catch (error) { reply({error: error.message || 'Text could not be saved on this device.'}); }
}

async function submitAction(projectId, action) {
  const requestId = crypto.randomUUID();
  try {
    await api(`/api/projects/${projectId}/operations`, {method: 'POST', headers: {'content-type': 'application/json'}, body: JSON.stringify({action, request_id: requestId})});
    await refreshProjects();
  } catch (error) {
    // An acknowledgement can be lost after admission. Observe; never replay.
    try {
      const snapshot = await api(`/api/projects/${projectId}/operations`);
      const accepted = [snapshot.active, ...snapshot.recent].find(record => record?.request_id === requestId);
      if (!accepted) notify(`${error.message} Refresh status before trying another action.`, true);
    } catch { notify('Cannot confirm this action. Reconnect and check Project status before trying again.', true); }
    await refreshProjects();
  }
}

async function renderSavedText() {
  const list = $('saved-text-list');
  const pending = await textStore.readPending();
  list.replaceChildren();
  if (!pending.length) {
    const empty = document.createElement('p');
    empty.textContent = 'All saved text has been accepted.';
    list.append(empty);
  }
  for (const record of pending) {
    const project = projects.find(item => item.id === record.projectId);
    const item = document.createElement('article');
    const title = document.createElement('h3');
    title.textContent = project?.name || 'Project no longer registered';
    const status = document.createElement('p');
    status.textContent = record.state === 'blocked' ? record.error : project?.dsh_status === 'running' ? 'Waiting for confirmation from DSH.' : 'Waiting for this Project’s DSH Runtime.';
    const text = document.createElement('textarea');
    text.readOnly = true;
    text.rows = 4;
    text.value = record.text;
    text.setAttribute('aria-label', `Saved text for ${title.textContent}`);
    const actions = document.createElement('div');
    actions.className = 'saved-text-actions';
    const select = document.createElement('button');
    select.className = 'text-button';
    select.textContent = 'Select text';
    select.onclick = () => { text.focus(); text.select(); };
    actions.append(select);
    if (project) {
      const open = document.createElement('button');
      open.className = 'text-button';
      open.textContent = 'Open conversation';
      open.onclick = () => { $('saved-text-dialog').close(); navigate(record.projectId, record.sessionId); };
      actions.append(open);
    }
    if (record.state === 'blocked') {
      const retry = document.createElement('button');
      retry.className = 'secondary-button';
      retry.textContent = 'Try again';
      retry.onclick = async () => {
        try {
          await textStore.retryText(record.projectId, record.sessionId, record.requestId);
          await reconcileFrames();
          await renderSavedText();
        } catch (error) { notify(error.message, true); }
      };
      actions.append(retry);
    }
    item.append(title, status, text, actions);
    list.append(item);
  }
}

function confirmAction(title, description, label, action) {
  $('confirmation-title').textContent = title;
  $('confirmation-description').textContent = description;
  $('confirm-action').textContent = label;
  $('confirm-action').onclick = () => { $('confirmation-dialog').close(); action(); };
  $('confirmation-dialog').showModal();
}

function renderActions(project) {
  $('actions-title').textContent = project.name;
  $('actions-path').textContent = project.path;
  const details = $('project-details');
  details.replaceChildren();
  for (const [label, status] of [['VM', project.vm_status], ['DSH', project.dsh_status], ['Sync', project.sync_status || 'not configured']]) {
    const item = document.createElement('span');
    const value = document.createElement('strong');
    value.textContent = status.replaceAll('_', ' ');
    item.append(`${label} `, value);
    details.append(item);
  }
  const result = project.operation || project.last_operation;
  $('operation-status').hidden = !result;
  $('operation-status').textContent = result ? `${result.action}: ${result.state.replaceAll('_', ' ')}${result.error ? `. ${result.error}` : ''}` : '';
  const list = $('action-list');
  list.replaceChildren();
  const active = Boolean(project.operation);
  const actions = [
    ['launch_dsh', project.dsh_status === 'running' ? 'Open conversation' : 'Launch DSH', false],
    ['restart_dsh', 'Restart DSH', false],
    ['stop_dsh', 'Stop DSH', true],
    ['start_vm', 'Start VM', false],
    ['stop_vm', 'Stop VM', true],
    ['delete_vm', 'Delete VM', true],
  ];
  for (const [action, label, dangerous] of actions) {
    const button = document.createElement('button');
    button.textContent = label;
    if (dangerous) button.className = 'danger';
    const stopping = ['stop_vm', 'stop_dsh', 'delete_vm'].includes(action);
    button.disabled = active && (!stopping || ['VM stop', 'VM delete', 'DSH stop'].includes(project.operation.action));
    if (action === 'restart_dsh' && project.vm_status !== 'running') button.disabled = true;
    button.onclick = () => {
      if (action === 'launch_dsh' && project.dsh_status === 'running') { $('actions-dialog').close(); navigate(project.id); return; }
      if (dangerous || action === 'restart_dsh') confirmAction(`${label}?`, action === 'delete_vm' ? 'This deletes the Project VM. Registered workspace files and Project Logs remain. Pending text will wait until DSH is running again.' : 'This interrupts the Project runtime. Drafts and pending text stay associated with this Project.', label, () => submitAction(project.id, action));
      else submitAction(project.id, action);
    };
    list.append(button);
  }
}

function openActions() {
  const project = projects.find(item => item.id === selectedProject);
  if (!project) return;
  renderActions(project);
  $('actions-dialog').showModal();
}

function openManagement() {
  $('actions-dialog').close();
  setProjectsOpen(false);
  if (!$('management-frame').src) $('management-frame').src = '/manage';
  $('management-dialog').showModal();
}

async function refreshLogs() {
  if (logsRequest || !logsProject) return logsRequest;
  logsRequest = (async () => {
    try {
      const logs = await api(`/api/projects/${logsProject}/logs`);
      const content = $('logs-content');
      const following = content.scrollTop + content.clientHeight >= content.scrollHeight - 40;
      const text = logs.entries.map(entry => `${entry.ts} [${entry.source}] ${entry.message}`).join('\n');
      if (content.textContent !== text) content.textContent = text || 'No Project Logs yet.';
      if (following) content.scrollTop = content.scrollHeight;
      $('logs-status').textContent = 'Up to date';
    } catch (error) { $('logs-status').textContent = error.message; }
    finally { logsRequest = null; }
  })();
  return logsRequest;
}

$('open-projects').onclick = () => setProjectsOpen(app.dataset.projectsOpen !== 'true');
$('project-title').onclick = () => setProjectsOpen(true);
$('close-projects').onclick = () => { setProjectsOpen(false); $('open-projects').focus(); };
$('projects-backdrop').onclick = () => setProjectsOpen(false);
$('refresh-projects').onclick = async () => {
  const button = $('refresh-projects');
  button.disabled = true;
  button.setAttribute('aria-busy', 'true');
  try {
    const refreshed = await refreshProjects();
    notify(refreshed ? 'Project status refreshed.' : 'Could not refresh Project status. Control Daemon unavailable.', !refreshed);
  } finally {
    button.disabled = false;
    button.removeAttribute('aria-busy');
  }
};
$('project-search').oninput = renderProjects;
$('manage-projects').onclick = openManagement;
$('open-management').onclick = openManagement;
$('project-actions').onclick = openActions;
$('placeholder-action').onclick = () => placeholderAction?.();
$('dismiss-notice').onclick = () => { $('notice').hidden = true; };
$('connection-status').onclick = () => {
  const frame = frames.get(selectedProject);
  if (!frame?.ready) return;
  if (frame.connection === 'auth-required') reauthorize(frame, true);
  else post(frame, 'foreground');
};
$('open-logs').onclick = () => {
  logsProject = selectedProject;
  $('logs-description').textContent = projects.find(item => item.id === logsProject)?.name || '';
  $('logs-content').textContent = 'Loading Project Logs…';
  $('logs-dialog').showModal();
  refreshLogs();
};
$('refresh-logs').onclick = refreshLogs;
$('open-saved-text').onclick = () => {
  $('saved-text-dialog').showModal();
  renderSavedText().catch(error => notify(error.message, true));
};
$('cancel-confirmation').onclick = () => $('confirmation-dialog').close();
for (const button of document.querySelectorAll('[data-close-dialog]')) button.onclick = () => button.closest('dialog').close();
$('management-dialog').addEventListener('close', refreshProjects);
window.addEventListener('message', event => { receiveBridge(event).catch(error => notify(error.message, true)); });
window.addEventListener('popstate', () => { const target = route(); navigate(target.project, target.session, 'none'); });
window.addEventListener('online', refreshProjects);
window.addEventListener('pageshow', refreshProjects);
window.addEventListener('keydown', event => { if (event.key === 'Escape') setProjectsOpen(false); });
wideLayout.addEventListener('change', () => setProjectsOpen(wideLayout.matches));
document.addEventListener('visibilitychange', () => {
  if (!document.hidden) {
    refreshProjects();
    for (const frame of frames.values()) if (frame.ready) post(frame, 'foreground');
  }
});
setInterval(() => { if (!document.hidden && navigator.onLine) refreshProjects(); }, 500);
setInterval(() => { if ($('logs-dialog').open && !document.hidden && navigator.onLine) refreshLogs(); }, 2000);
setProjectsOpen(wideLayout.matches);
await refreshProjects();
const initial = route();
if (initial.project) await navigate(initial.project, initial.session, 'none');
if ('serviceWorker' in navigator) {
  navigator.serviceWorker.register('/service-worker.js').then(registration => {
    const reportUpdate = () => {
      if (registration.waiting && navigator.serviceWorker.controller) notify('An app update is ready. Close all DevVM windows and reopen to use it.');
    };
    reportUpdate();
    registration.addEventListener('updatefound', () => registration.installing?.addEventListener('statechange', reportUpdate));
  }).catch(error => notify(`Offline app shell unavailable: ${error.message}`, true));
}
