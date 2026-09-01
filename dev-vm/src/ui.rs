pub const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>DevVM Workspace Supervision</title>
    <style>
        :root {
            color-scheme: light;
            --bg-color: #f8fafc;
            --card-bg: #ffffff;
            --card-border: #e2e8f0;
            --text-color: #0f172a;
            --text-muted: #64748b;
            --accent: #2563eb;
            --accent-hover: #1d4ed8;
            --success: #15803d;
            --warning: #b45309;
            --danger: #dc2626;
            --danger-hover: #b91c1c;
            --btn-secondary: #ffffff;
            --btn-secondary-hover: #f1f5f9;
            --surface-muted: #f8fafc;
            --focus-ring: rgba(37, 99, 235, 0.18);
            --shadow: 0 1px 2px rgba(15, 23, 42, 0.04), 0 8px 24px rgba(15, 23, 42, 0.04);
        }

        * {
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }

        body {
            font-family: Inter, ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
            background-color: var(--bg-color);
            color: var(--text-color);
            line-height: 1.5;
            min-height: 100vh;
            padding: 32px 24px 64px;
        }

        .container {
            max-width: 1280px;
            margin: 0 auto;
        }

        header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            gap: 16px;
            margin-bottom: 24px;
        }

        h1 {
            font-size: 1.35rem;
            font-weight: 650;
            letter-spacing: -0.02em;
        }

        .btn {
            display: inline-flex;
            align-items: center;
            justify-content: center;
            gap: 6px;
            background-color: var(--accent);
            color: #ffffff;
            font-weight: 600;
            border: 1px solid transparent;
            padding: 8px 14px;
            border-radius: 8px;
            cursor: pointer;
            font-size: 0.875rem;
            line-height: 1.25;
            transition: background-color 0.15s ease, border-color 0.15s ease, box-shadow 0.15s ease;
            text-decoration: none;
        }

        .btn:hover {
            background-color: var(--accent-hover);
        }

        .btn:focus-visible,
        .port-input:focus,
        .form-control:focus {
            outline: none;
            border-color: var(--accent);
            box-shadow: 0 0 0 3px var(--focus-ring);
        }

        .btn:disabled {
            cursor: wait;
            opacity: 0.62;
        }

        .btn-secondary {
            background-color: var(--btn-secondary);
            color: #334155;
            border-color: var(--card-border);
        }

        .btn-secondary:hover {
            background-color: var(--btn-secondary-hover);
            border-color: #cbd5e1;
        }

        .btn-success {
            background-color: #16a34a;
            color: #ffffff;
        }

        .btn-success:hover {
            background-color: var(--success);
        }

        .btn-danger {
            background-color: var(--danger);
            color: white;
        }

        .btn-danger:hover {
            background-color: var(--danger-hover);
        }

        .btn-sm {
            padding: 7px 11px;
            font-size: 0.8125rem;
        }

        .projects-grid {
            display: grid;
            gap: 16px;
        }

        .project-card {
            background-color: var(--card-bg);
            border: 1px solid var(--card-border);
            border-radius: 12px;
            padding: 24px;
            display: flex;
            flex-direction: column;
            gap: 20px;
            box-shadow: var(--shadow);
        }

        .project-header {
            display: flex;
            justify-content: space-between;
            align-items: flex-start;
            flex-wrap: wrap;
            gap: 8px;
        }

        .project-title {
            font-size: 1.125rem;
            font-weight: 650;
            color: var(--text-color);
            letter-spacing: -0.01em;
            margin-bottom: 4px;
        }

        .project-meta {
            font-size: 0.8125rem;
            color: var(--text-muted);
            word-break: break-all;
        }

        .badges {
            display: flex;
            gap: 8px;
            flex-wrap: wrap;
            align-items: center;
        }

        .badge {
            display: inline-flex;
            align-items: center;
            gap: 6px;
            font-size: 0.75rem;
            padding: 5px 9px;
            border-radius: 999px;
            font-weight: 650;
            letter-spacing: 0.01em;
            border: 1px solid transparent;
        }

        .badge-running,
        .badge-sync-synchronized { background-color: #f0fdf4; color: #15803d; border-color: #bbf7d0; }
        .badge-stopped,
        .badge-sync-not_configured { background-color: #f8fafc; color: #64748b; border-color: #e2e8f0; }
        .badge-failed,
        .badge-sync-failed { background-color: #fef2f2; color: #b91c1c; border-color: #fecaca; }
        .badge-starting,
        .badge-stopping,
        .badge-sync-synchronizing { background-color: #eff6ff; color: #1d4ed8; border-color: #bfdbfe; }
        .badge-sync-degraded { background-color: #fffbeb; color: #b45309; border-color: #fde68a; }

        .spinner {
            width: 11px;
            height: 11px;
            border: 1.5px solid currentColor;
            border-right-color: transparent;
            border-radius: 50%;
            animation: spin 0.7s linear infinite;
        }

        @keyframes spin {
            to { transform: rotate(360deg); }
        }

        .project-actions {
            display: flex;
            gap: 8px;
            flex-wrap: wrap;
            border-top: 1px solid var(--card-border);
            padding-top: 20px;
        }

        .open-port-row {
            display: flex;
            align-items: center;
            gap: 8px;
            flex-wrap: wrap;
            padding: 12px;
            background-color: var(--surface-muted);
            border-radius: 9px;
            border: 1px solid var(--card-border);
        }

        .open-port-row label {
            font-size: 0.85rem;
            color: var(--text-muted);
            font-weight: 500;
        }

        .port-input {
            background-color: #ffffff;
            border: 1px solid var(--card-border);
            color: var(--text-color);
            padding: 7px 10px;
            border-radius: 7px;
            font-size: 0.8125rem;
            width: 112px;
        }

        .port-links-container {
            display: flex;
            gap: 6px;
            align-items: center;
        }

        .empty-state {
            text-align: center;
            padding: 48px;
            background-color: var(--card-bg);
            border: 1px dashed var(--card-border);
            border-radius: 8px;
            color: var(--text-muted);
        }

        /* Modal styles */
        .modal-overlay {
            display: none;
            position: fixed;
            top: 0;
            left: 0;
            right: 0;
            bottom: 0;
            background-color: rgba(15, 23, 42, 0.38);
            backdrop-filter: blur(2px);
            z-index: 1000;
            justify-content: center;
            align-items: center;
            padding: 20px;
        }

        .modal {
            background-color: var(--card-bg);
            border: 1px solid var(--card-border);
            border-radius: 12px;
            width: 100%;
            max-width: 600px;
            display: flex;
            flex-direction: column;
            max-height: 80vh;
            box-shadow: 0 24px 64px rgba(15, 23, 42, 0.18);
        }

        .modal-header {
            padding: 16px 20px;
            border-bottom: 1px solid var(--card-border);
            display: flex;
            justify-content: space-between;
            align-items: center;
        }

        .modal-header h2 {
            font-size: 1.2rem;
            font-weight: 600;
        }

        .close-btn {
            background: none;
            border: none;
            color: var(--text-muted);
            font-size: 1.5rem;
            cursor: pointer;
            line-height: 1;
        }

        .close-btn:hover {
            color: var(--text-color);
        }

        .modal-body {
            padding: 20px;
            overflow-y: auto;
            flex: 1;
        }

        .modal-footer {
            padding: 16px 20px;
            border-top: 1px solid var(--card-border);
            display: flex;
            justify-content: flex-end;
            gap: 10px;
        }

        .browser-path {
            background-color: var(--surface-muted);
            padding: 8px 12px;
            border: 1px solid var(--card-border);
            border-radius: 8px;
            font-family: monospace;
            font-size: 0.9rem;
            margin-bottom: 12px;
            word-break: break-all;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }

        .browser-list {
            list-style: none;
            border: 1px solid var(--card-border);
            border-radius: 4px;
            overflow: hidden;
        }

        .browser-item {
            padding: 10px 12px;
            border-bottom: 1px solid var(--card-border);
            cursor: pointer;
            display: flex;
            align-items: center;
            gap: 8px;
            transition: background 0.1s ease;
        }

        .browser-item:last-child {
            border-bottom: none;
        }

        .browser-item:hover {
            background-color: var(--surface-muted);
        }

        .logs-pre {
            background-color: var(--surface-muted);
            color: #334155;
            border: 1px solid var(--card-border);
            font-family: "SFMono-Regular", Consolas, "Liberation Mono", monospace;
            font-size: 0.8125rem;
            line-height: 1.6;
            padding: 16px;
            border-radius: 9px;
            overflow-x: auto;
            max-height: 500px;
            white-space: pre-wrap;
            word-break: break-all;
        }

        .form-group {
            margin-bottom: 16px;
        }

        .form-group label {
            display: block;
            font-size: 0.85rem;
            color: var(--text-muted);
            margin-bottom: 6px;
            font-weight: 500;
        }

        .form-control {
            width: 100%;
            background-color: #ffffff;
            border: 1px solid var(--card-border);
            color: var(--text-color);
            padding: 9px 11px;
            border-radius: 8px;
            font-size: 0.875rem;
        }

        @media (max-width: 720px) {
            body { padding: 20px 14px 40px; }
            header { align-items: flex-start; flex-direction: column; }
            .project-card { padding: 18px; }
            .badges { margin-top: 4px; }
        }
    </style>
</head>
<body>
    <div class="container">
        <header>
            <div>
                <h1>DevVM Control Daemon</h1>
            </div>
            <div style="display: flex; gap: 10px;">
                <button class="btn btn-secondary" onclick="openSyncSetupModal()">Sync Setup</button>
                <button class="btn btn-secondary" onclick="fetchProjects()">Refresh</button>
                <button class="btn" onclick="openBrowser()">+ Register Project</button>
            </div>
        </header>

        <div id="projects-container" class="projects-grid">
            <div class="empty-state">Loading registered projects...</div>
        </div>
    </div>

    <!-- Project Browser Modal -->
    <div id="browser-modal" class="modal-overlay">
        <div class="modal">
            <div class="modal-header">
                <h2>Select Project Directory</h2>
                <button class="close-btn" onclick="closeModal('browser-modal')">&times;</button>
            </div>
            <div class="modal-body">
                <div class="browser-path">
                    <span id="current-browser-path">/</span>
                    <button id="browser-up-btn" class="btn btn-secondary btn-sm" onclick="browseUp()">Up</button>
                </div>
                <ul id="browser-list" class="browser-list">
                    <li class="browser-item">Loading directories...</li>
                </ul>
            </div>
            <div class="modal-footer">
                <button class="btn btn-secondary" onclick="closeModal('browser-modal')">Cancel</button>
                <button class="btn" onclick="registerCurrentPath()">Register This Directory</button>
            </div>
        </div>
    </div>

    <!-- Sync Setup Modal -->
    <div id="sync-modal" class="modal-overlay">
        <div class="modal">
            <div class="modal-header">
                <h2>Portable DSH State Sync Configuration</h2>
                <button class="close-btn" onclick="closeModal('sync-modal')">&times;</button>
            </div>
            <div class="modal-body">
                <div class="form-group">
                    <label for="sync-ssh-user">SSH User:</label>
                    <input type="text" id="sync-ssh-user" class="form-control" placeholder="e.g. ubuntu or devvm" />
                </div>
                <div class="form-group">
                    <label for="sync-ssh-host">SSH Host / IP:</label>
                    <input type="text" id="sync-ssh-host" class="form-control" placeholder="e.g. vps.example.com" />
                </div>
                <div class="form-group">
                    <label for="sync-ssh-port">SSH Port:</label>
                    <input type="number" id="sync-ssh-port" class="form-control" value="22" />
                </div>
                <div class="form-group">
                    <label for="sync-ssh-key">SSH Private Key Path:</label>
                    <input type="text" id="sync-ssh-key" class="form-control" placeholder="e.g. /root/.ssh/id_ed25519" />
                </div>
                <div class="form-group">
                    <label for="sync-remote-root">Remote Sync Root Directory:</label>
                    <input type="text" id="sync-remote-root" class="form-control" value="/var/lib/devvm-sync" />
                </div>
                <div class="form-group" style="display: flex; align-items: center; gap: 8px;">
                    <input type="checkbox" id="sync-verify" checked />
                    <label for="sync-verify" style="margin-bottom: 0;">Verify SSH connectivity before saving</label>
                </div>
                <div id="sync-setup-msg" style="font-size: 0.85rem; margin-top: 8px;"></div>
            </div>
            <div class="modal-footer">
                <button class="btn btn-secondary" onclick="closeModal('sync-modal')">Cancel</button>
                <button class="btn" onclick="saveSyncSetup()">Save Configuration</button>
            </div>
        </div>
    </div>

    <!-- Logs Modal -->
    <div id="logs-modal" class="modal-overlay">
        <div class="modal" style="max-width: 900px;">
            <div class="modal-header">
                <h2 id="logs-title">Project Logs</h2>
                <button class="close-btn" onclick="closeModal('logs-modal')">&times;</button>
            </div>
            <div class="modal-body">
                <pre id="logs-content" class="logs-pre">Loading logs...</pre>
            </div>
            <div class="modal-footer">
                <button class="btn btn-secondary" onclick="refreshCurrentLogs()">Refresh Logs</button>
                <button class="btn" onclick="closeModal('logs-modal')">Close</button>
            </div>
        </div>
    </div>

    <script>
        let currentPath = '';
        let currentParent = null;
        let activeLogProjectId = null;
        let logRefreshTimer = null;
        let currentProjects = [];
        const pendingActions = new Map();

        async function fetchProjects() {
            try {
                const res = await fetch('/api/projects');
                if (!res.ok) throw new Error('Failed to fetch projects');
                const projects = await res.json();
                currentProjects = projects;
                renderProjects(projects);
            } catch (err) {
                console.error(err);
                document.getElementById('projects-container').innerHTML = 
                    `<div class="empty-state" style="color: var(--danger)">Error loading projects: ${err.message}</div>`;
            }
        }

        function setPendingAction(id, statuses) {
            pendingActions.set(id, { ...(pendingActions.get(id) || {}), ...statuses });
            renderProjects(currentProjects);
        }

        function clearPendingAction(id, statusKeys) {
            const pending = { ...(pendingActions.get(id) || {}) };
            statusKeys.forEach(key => delete pending[key]);
            if (Object.keys(pending).length === 0) {
                pendingActions.delete(id);
            } else {
                pendingActions.set(id, pending);
            }
        }

        function statusPresentation(project, type) {
            const value = pendingActions.get(project.id)?.[type] || project[`${type}_status`];
            const label = value.charAt(0).toUpperCase() + value.slice(1).replace('_', ' ');
            return {
                value,
                label,
                loading: value === 'starting' || value === 'stopping',
            };
        }

        async function runLifecycleAction(id, statuses, url, errorLabel) {
            setPendingAction(id, statuses);
            try {
                const res = await fetch(url, { method: 'POST' });
                if (!res.ok) {
                    const error = await res.json().catch(() => ({}));
                    throw new Error(error.error || `Failed to ${errorLabel}`);
                }
            } catch (error) {
                alert(`Error ${errorLabel}: ${error.message}`);
            } finally {
                clearPendingAction(id, Object.keys(statuses));
                await fetchProjects();
            }
        }

        function renderProjects(projects) {
            const container = document.getElementById('projects-container');
            if (!projects || projects.length === 0) {
                container.innerHTML = `
                    <div class="empty-state">
                        <p style="margin-bottom: 12px;">No projects registered yet.</p>
                        <button class="btn" onclick="openBrowser()">Register Your First Project</button>
                    </div>
                `;
                return;
            }

            container.innerHTML = projects.map(p => {
                const vmStatus = statusPresentation(p, 'vm');
                const dshStatus = statusPresentation(p, 'dsh');
                const vmSpinner = vmStatus.loading ? '<span class="spinner" aria-hidden="true"></span>' : '';
                const dshSpinner = dshStatus.loading ? '<span class="spinner" aria-hidden="true"></span>' : '';
                const vmBadge = `<span class="badge badge-${vmStatus.value}">${vmSpinner}VM: ${vmStatus.label}</span>`;
                const dshBadge = `<span class="badge badge-${dshStatus.value}">${dshSpinner}DSH: ${dshStatus.label}</span>`;

                const syncStatus = p.sync_status || 'not_configured';
                const syncSpinner = syncStatus === 'synchronizing' ? '<span class="spinner" aria-hidden="true"></span>' : '';
                const syncLabel = syncStatus.charAt(0).toUpperCase() + syncStatus.slice(1).replace('_', ' ');
                const syncBadge = `<span class="badge badge-sync-${syncStatus}">${syncSpinner}Sync: ${syncLabel}</span>`;

                const localDshUrl = p.links && (p.links.local_dsh_url || p.links.dsh_url);
                const tailnetDshUrl = p.links && p.links.tailnet_dsh_url;
                const dshReady = dshStatus.value === 'running';

                const localDshLink = localDshUrl && dshReady
                    ? `<a href="${localDshUrl}" target="_blank" class="btn btn-sm btn-success">Open DSH (Local)</a>`
                    : '';
                const tailnetDshLink = tailnetDshUrl && dshReady
                    ? `<a href="${tailnetDshUrl}" target="_blank" class="btn btn-secondary btn-sm">Open DSH (Tailnet)</a>`
                    : '';

                const vmActionBtn = vmStatus.loading
                    ? `<button class="btn btn-secondary btn-sm" disabled>${vmSpinner}${vmStatus.label} VM</button>`
                    : vmStatus.value === 'running'
                        ? `<button class="btn btn-secondary btn-sm" onclick="stopVm('${p.id}')">Stop VM</button>`
                        : `<button class="btn btn-secondary btn-sm" onclick="startVm('${p.id}')">Start VM</button>`;

                const dshActionBtn = dshStatus.loading
                    ? `<button class="btn btn-secondary btn-sm" disabled>${dshSpinner}${dshStatus.label} DSH</button>`
                    : dshStatus.value === 'running'
                        ? `<button class="btn btn-secondary btn-sm" onclick="stopDsh('${p.id}')">Stop DSH</button>`
                        : `<button class="btn btn-secondary btn-sm" onclick="launchDsh('${p.id}')">Launch DSH</button>`;

                return `
                    <div class="project-card" id="card-${p.id}">
                        <div class="project-header">
                            <div>
                                <div class="project-title">${escapeHtml(p.name)}</div>
                                <div class="project-meta">Path: ${escapeHtml(p.path)}</div>
                                <div class="project-meta">ID: ${p.id} &bull; Host: ${p.project_host}</div>
                            </div>
                            <div class="badges">
                                ${vmBadge}
                                ${dshBadge}
                                ${syncBadge}
                            </div>
                        </div>

                        <div class="open-port-row">
                            <label for="port-input-${p.id}">Open Port:</label>
                            <input type="number" id="port-input-${p.id}" placeholder="e.g. 3000" min="1" max="65535" class="port-input" onkeydown="if(event.key==='Enter') openProjectPort('${p.id}')" />
                            <button class="btn btn-secondary btn-sm" onclick="openProjectPort('${p.id}')">Open</button>
                            <div id="port-links-${p.id}" class="port-links-container"></div>
                        </div>

                        <div class="project-actions">
                            ${localDshLink}
                            ${tailnetDshLink}
                            ${dshActionBtn}
                            ${vmActionBtn}
                            <button class="btn btn-secondary btn-sm" onclick="triggerProjectSync('${p.id}')">Sync Now</button>
                            <button class="btn btn-secondary btn-sm" onclick="viewLogs('${p.id}', '${escapeHtml(p.name)}')">View Logs</button>
                            <button class="btn btn-secondary btn-sm" onclick="unregisterProject('${p.id}')">Unregister</button>
                            <button class="btn btn-secondary btn-sm" onclick="deleteProjectSync('${p.id}', '${escapeHtml(p.name)}')">Delete Sync Store</button>
                            <button class="btn btn-danger btn-sm" onclick="deleteVm('${p.id}', '${escapeHtml(p.name)}')">Delete VM</button>
                        </div>
                    </div>
                `;
            }).join('');
        }

        async function openProjectPort(projectId) {
            const input = document.getElementById(`port-input-${projectId}`);
            const linksContainer = document.getElementById(`port-links-${projectId}`);
            if (!input || !linksContainer) return;

            const port = parseInt(input.value, 10);
            if (!port || port <= 0 || port > 65535) {
                alert('Please enter a valid port number between 1 and 65535');
                return;
            }

            try {
                const res = await fetch(`/api/projects/${projectId}/open-port`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ port }),
                });
                if (!res.ok) {
                    const err = await res.json().catch(() => ({}));
                    throw new Error(err.error || 'Failed to open port');
                }
                const data = await res.json();
                linksContainer.innerHTML = `
                    <a href="${data.local_url}" target="_blank" class="btn btn-sm" style="background-color: var(--accent); color: #0f172a;">Local :${port}</a>
                    <a href="${data.tailnet_url}" target="_blank" class="btn btn-secondary btn-sm">Tailnet :${port}</a>
                `;
            } catch (err) {
                alert('Error opening port: ' + err.message);
            }
        }

        async function openBrowser(path) {
            document.getElementById('browser-modal').style.display = 'flex';
            loadBrowserPath(path || '');
        }

        async function loadBrowserPath(path) {
            const listEl = document.getElementById('browser-list');
            listEl.innerHTML = '<li class="browser-item">Loading...</li>';
            try {
                const url = path ? `/api/browser?path=${encodeURIComponent(path)}` : '/api/browser';
                const res = await fetch(url);
                if (!res.ok) {
                    const errData = await res.json().catch(() => ({}));
                    throw new Error(errData.error || 'Failed to browse directory');
                }
                const data = await res.json();
                currentPath = data.current;
                currentParent = data.parent;

                document.getElementById('current-browser-path').textContent = currentPath;
                document.getElementById('browser-up-btn').style.display = currentParent ? 'inline-block' : 'none';

                if (data.entries.length === 0) {
                    listEl.innerHTML = '<li class="browser-item" style="color: var(--text-muted);">No subdirectories found</li>';
                    return;
                }

                listEl.innerHTML = data.entries.map(e => `
                    <li class="browser-item" onclick="loadBrowserPath('${escapeJs(e.path)}')">
                        📁 ${escapeHtml(e.name)}
                    </li>
                `).join('');
            } catch (err) {
                listEl.innerHTML = `<li class="browser-item" style="color: var(--danger)">${escapeHtml(err.message)}</li>`;
            }
        }

        function browseUp() {
            if (currentParent) {
                loadBrowserPath(currentParent);
            }
        }

        async function registerCurrentPath() {
            if (!currentPath) return;
            try {
                const res = await fetch('/api/projects/register', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ path: currentPath }),
                });
                if (!res.ok) {
                    const err = await res.json().catch(() => ({}));
                    throw new Error(err.error || 'Failed to register project');
                }
                closeModal('browser-modal');
                fetchProjects();
            } catch (err) {
                alert('Error registering project: ' + err.message);
            }
        }

        async function unregisterProject(id) {
            if (!confirm('Unregister this project from the Control Daemon? (DevVM and files will not be deleted)')) return;
            try {
                const res = await fetch(`/api/projects/${id}/unregister`, { method: 'POST' });
                if (!res.ok) throw new Error('Failed to unregister');
                fetchProjects();
            } catch (err) {
                alert('Error unregistering: ' + err.message);
            }
        }

        async function startVm(id) {
            await runLifecycleAction(id, { vm: 'starting' }, `/api/projects/${id}/vm/start`, 'starting VM');
        }

        async function stopVm(id) {
            const project = currentProjects.find(item => item.id === id);
            const statuses = project?.dsh_status === 'running'
                ? { vm: 'stopping', dsh: 'stopping' }
                : { vm: 'stopping' };
            await runLifecycleAction(id, statuses, `/api/projects/${id}/vm/stop`, 'stopping VM');
        }

        async function deleteVm(id, name) {
            if (!confirm(`Delete DevVM for "${name}"? This will delete the VM instance.`)) return;
            try {
                const res = await fetch(`/api/projects/${id}/vm/delete`, { method: 'POST' });
                if (!res.ok) {
                    const err = await res.json().catch(() => ({}));
                    throw new Error(err.error || 'Failed to delete VM');
                }
                fetchProjects();
            } catch (err) {
                alert('Error deleting VM: ' + err.message);
            }
        }

        async function launchDsh(id) {
            const project = currentProjects.find(item => item.id === id);
            const statuses = project?.vm_status === 'running'
                ? { dsh: 'starting' }
                : { vm: 'starting', dsh: 'starting' };
            await runLifecycleAction(id, statuses, `/api/projects/${id}/dsh/launch`, 'launching DSH');
        }

        async function stopDsh(id) {
            await runLifecycleAction(id, { dsh: 'stopping' }, `/api/projects/${id}/dsh/stop`, 'stopping DSH');
        }

        async function triggerProjectSync(id) {
            try {
                const res = await fetch(`/api/projects/${id}/sync/trigger`, { method: 'POST' });
                if (!res.ok) {
                    const err = await res.json().catch(() => ({}));
                    throw new Error(err.error || 'Failed to trigger sync');
                }
                fetchProjects();
            } catch (err) {
                alert('Error triggering sync: ' + err.message);
            }
        }

        async function deleteProjectSync(id, name) {
            if (!confirm(`Delete remote Sync Store data for project "${name}"? This will remove synchronization data from the Sync Store.`)) return;
            try {
                const res = await fetch(`/api/projects/${id}/sync/delete`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ confirmed: true }),
                });
                if (!res.ok) {
                    const err = await res.json().catch(() => ({}));
                    throw new Error(err.error || 'Failed to delete remote sync store');
                }
                alert('Remote Sync Store data deleted successfully.');
                fetchProjects();
            } catch (err) {
                alert('Error deleting remote sync store: ' + err.message);
            }
        }

        async function openSyncSetupModal() {
            document.getElementById('sync-modal').style.display = 'flex';
            const msgEl = document.getElementById('sync-setup-msg');
            msgEl.textContent = 'Loading existing configuration...';
            msgEl.style.color = 'var(--text-muted)';
            try {
                const res = await fetch('/api/sync/config');
                if (res.ok) {
                    const data = await res.json();
                    if (data.configured && data.config) {
                        document.getElementById('sync-ssh-user').value = data.config.ssh_user || '';
                        document.getElementById('sync-ssh-host').value = data.config.ssh_host || '';
                        document.getElementById('sync-ssh-port').value = data.config.ssh_port || 22;
                        document.getElementById('sync-ssh-key').value = data.config.ssh_key_path || '';
                        document.getElementById('sync-remote-root').value = data.config.remote_sync_root || '/var/lib/devvm-sync';
                        msgEl.textContent = 'Current configuration loaded.';
                    } else {
                        msgEl.textContent = 'No sync configuration currently set.';
                    }
                }
            } catch (err) {
                msgEl.textContent = 'Failed to load config: ' + err.message;
            }
        }

        async function saveSyncSetup() {
            const msgEl = document.getElementById('sync-setup-msg');
            const ssh_user = document.getElementById('sync-ssh-user').value.trim();
            const ssh_host = document.getElementById('sync-ssh-host').value.trim();
            const ssh_port = parseInt(document.getElementById('sync-ssh-port').value, 10) || 22;
            const ssh_key_path = document.getElementById('sync-ssh-key').value.trim();
            const remote_sync_root = document.getElementById('sync-remote-root').value.trim();
            const verify = document.getElementById('sync-verify').checked;

            if (!ssh_user || !ssh_host || !ssh_key_path || !remote_sync_root) {
                msgEl.textContent = 'Please fill out all required fields.';
                msgEl.style.color = 'var(--danger)';
                return;
            }

            msgEl.textContent = verify ? 'Verifying SSH connection...' : 'Saving configuration...';
            msgEl.style.color = 'var(--accent)';

            try {
                const res = await fetch('/api/sync/setup', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        ssh_user,
                        ssh_host,
                        ssh_port,
                        ssh_key_path,
                        remote_sync_root,
                        verify
                    })
                });

                if (!res.ok) {
                    const err = await res.json().catch(() => ({}));
                    throw new Error(err.error || 'Failed to save sync config');
                }

                msgEl.textContent = 'Sync configuration saved successfully!';
                msgEl.style.color = 'var(--success)';
                setTimeout(() => {
                    closeModal('sync-modal');
                    fetchProjects();
                }, 1000);
            } catch (err) {
                msgEl.textContent = 'Error: ' + err.message;
                msgEl.style.color = 'var(--danger)';
            }
        }

        async function viewLogs(id, name) {
            activeLogProjectId = id;
            document.getElementById('logs-title').textContent = `Logs: ${name}`;
            document.getElementById('logs-modal').style.display = 'flex';
            await refreshCurrentLogs();
            clearInterval(logRefreshTimer);
            logRefreshTimer = setInterval(refreshCurrentLogs, 2000);
        }

        async function refreshCurrentLogs() {
            if (!activeLogProjectId) return;
            const logsEl = document.getElementById('logs-content');
            try {
                const res = await fetch(`/api/projects/${activeLogProjectId}/logs`);
                if (!res.ok) throw new Error('Failed to fetch logs');
                const data = await res.json();
                logsEl.textContent = data.logs || '(No logs recorded yet)';
                logsEl.scrollTop = logsEl.scrollHeight;
            } catch (err) {
                logsEl.textContent = 'Error loading logs: ' + err.message;
            }
        }

        function closeModal(id) {
            document.getElementById(id).style.display = 'none';
            if (id === 'logs-modal') {
                activeLogProjectId = null;
                clearInterval(logRefreshTimer);
                logRefreshTimer = null;
            }
        }

        function escapeHtml(str) {
            if (!str) return '';
            return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
        }

        function escapeJs(str) {
            if (!str) return '';
            return str.replace(/\\/g, '\\\\').replace(/'/g, "\\'");
        }

        // Initial load and periodic polling
        fetchProjects();
        setInterval(fetchProjects, 5000);
    </script>
</body>
</html>
"#;
