window.__ModuleLoader__.load({
  id: '@devvm/dsh-project-app',
  factory(require) {
    const {jsx} = require('react/jsx-runtime');
    const {Tooltip, IconPanelLeftOutlineRegular} = require('@deepseek-ai/dsh-client-ui-primitives');
    const inject = ['sessions', 'connection', 'conversation', 'uiWorkspace', 'layout', 'slots', 'remote', 'remote.session'];

    function apply(ctx) {
      const config = window.__DEVVM_EMBED__;
      if (window.parent === window || !config?.projectId || !config.controlOrigins.length) return;
      const instanceId = crypto.randomUUID();
      const commands = new Map();
      const drafts = new Map();
      const dispatches = new Map();
      const mainObservers = new Map();
      let attachment = null;
      let activeSession = null;
      let activeTitle = null;
      let disposed = false;
      let sequence = Promise.resolve();
      let authenticationProbe = null;

      function post(kind, payload = {}, commandId) {
        if (!attachment || disposed) return;
        parent.postMessage({protocol: 'devvm-embed', version: 1, projectId: config.projectId, channelId: attachment.channelId, instanceId, kind, payload, commandId}, attachment.origin);
      }

      function openNavigation() {
        ctx.layout.setSidebarOpen(true);
        post('navigation-opened');
      }

      function SidebarButton() {
        return jsx(Tooltip, {
          label: 'Open DSH sidebar',
          children: jsx('button', {
            type: 'button',
            className: 'IW6AQa_iconButton',
            'aria-label': 'Open DSH sidebar',
            onClick: openNavigation,
            children: jsx(IconPanelLeftOutlineRegular, {size: 18}),
          }),
        });
      }
      ctx.slots.inject('conversation.header.leading', () => ctx.slots.register({name: 'conversation.header.leading'}, SidebarButton));

      function request(kind, payload) {
        if (!attachment || disposed) return Promise.reject(new Error('Project text storage is disconnected.'));
        const commandId = crypto.randomUUID();
        return new Promise((resolve, reject) => {
          const timeout = setTimeout(() => { commands.delete(commandId); reject(new Error('Text could not be saved. Please keep this Project open and try again.')); }, 15000);
          commands.set(commandId, {resolve, reject, timeout, channelId: attachment.channelId});
          post(kind, payload, commandId);
        });
      }

      function connectionState() {
        return ctx.connection.state.getSnapshot() || 'connecting';
      }

      async function reportConnection() {
        const state = connectionState();
        post('connection-changed', {state});
        if (state === 'connected' || authenticationProbe) return;
        authenticationProbe = (async () => {
          try {
            const response = await fetch('/', {cache: 'no-store', credentials: 'same-origin'});
            if (response.status === 401) post('connection-changed', {state: 'auth-required'});
          } catch { /* An unreachable Runtime stays in native connection recovery. */ }
          finally { authenticationProbe = null; }
        })();
        await authenticationProbe;
      }

      async function reauthorize(url) {
        const target = new URL(url);
        if (target.origin !== location.origin || target.pathname !== '/' || target.searchParams.size !== 1 || !target.searchParams.get('token')) throw new Error('Invalid Project authorization URL.');
        const response = await fetch(target, {cache: 'no-store', credentials: 'same-origin', redirect: 'follow'});
        if (!response.ok) throw new Error(`Project authorization failed (${response.status}).`);
        ctx.connection.reconnect();
      }

      function reportSelection() {
        const catalog = ctx.sessions.list.getSnapshot();
        let sessionId = null;
        for (const id of new Set([...Object.keys(catalog.byId), ...drafts.keys()])) {
          if ((ctx.sessions.retainInfo(id).getSnapshot().retainedBy.mainView || 0) > 0) { sessionId = id; break; }
        }
        const title = sessionId ? catalog.byId[sessionId]?.title || '' : '';
        if (sessionId !== activeSession || title !== activeTitle) {
          activeSession = sessionId;
          activeTitle = title;
          post('selection-changed', {sessionId, title});
        }
      }

      function observeSelection() {
        const ids = new Set([...Object.keys(ctx.sessions.list.getSnapshot().byId), ...drafts.keys()]);
        for (const id of ids) if (!mainObservers.has(id)) mainObservers.set(id, ctx.sessions.retainInfo(id).subscribe(reportSelection));
        for (const [id, dispose] of mainObservers) if (!ids.has(id)) { dispose(); mainObservers.delete(id); }
        reportSelection();
      }

      const persistence = {
        async bind(session, input) {
          const sessionId = session.sessionId;
          const saved = await request('draft-load', {sessionId});
          input.setDraft(saved.text);
          if (input.state.getSnapshot().draft !== saved.text) await new Promise(resolve => {
            const unsubscribe = input.state.subscribe(() => {
              if (input.state.getSnapshot().draft === saved.text) { unsubscribe(); resolve(); }
            });
          });
          const entry = {input, session, text: saved.text, revision: saved.revision, saved: Promise.resolve(), unsubscribe: null};
          drafts.set(sessionId, entry);
          entry.unsubscribe = input.state.subscribe(() => {
            const text = input.state.getSnapshot().draft;
            if (text === entry.text) return;
            entry.text = text;
            const revision = ++entry.revision;
            const save = () => request('draft-save', {sessionId, text, revision});
            entry.saved = entry.saved.then(save, save);
            entry.saved.catch(error => post('error', {message: error.message}));
          });
          observeSelection();
          return () => {
            entry.unsubscribe();
            entry.saved.catch(error => post('error', {message: error.message}));
            if (drafts.get(sessionId) === entry) drafts.delete(sessionId);
          };
        },
        async admit(session, text, mode, requestId) {
          const entry = drafts.get(session.sessionId);
          if (!entry) throw new Error('Waiting for the saved draft.');
          await entry.saved;
          const receipt = await request('prompt-admit', {sessionId: session.sessionId, text, mode, requestId, draftRevision: entry.revision});
          entry.revision = receipt.revision;
          entry.text = ''; // The owner now commits this exact draft.
        },
        dispatch(session, text, mode, requestId, send) {
          const existing = dispatches.get(requestId);
          if (existing) return existing;
          const dispatch = (async () => {
            try {
              const result = await send();
              if (result.kind === 'success') await request('prompt-settled', {sessionId: session.sessionId, requestId});
              else if (connectionState() !== 'connected' || result.error?.code === 'gateway/internal') await request('prompt-waiting', {sessionId: session.sessionId, requestId});
              else await request('prompt-blocked', {sessionId: session.sessionId, requestId, error: result.error?.message || 'The Runtime refused this text.'});
              return result;
            } catch (error) {
              await request('prompt-waiting', {sessionId: session.sessionId, requestId});
              throw error;
            } finally { dispatches.delete(requestId); }
          })();
          dispatches.set(requestId, dispatch);
          return dispatch;
        },
      };

      async function flushDrafts() {
        await Promise.all([...drafts.values()].map(entry => entry.saved));
      }

      async function selectSession(sessionId) {
        if (typeof sessionId !== 'string') throw new Error('Invalid session identity.');
        await flushDrafts();
        const result = await ctx.remote.session.projections({sessionId});
        if (!result.ok) throw new Error(`Cannot verify this session: ${result.error.message}`);
        if (result.value === null) throw new Error('This session no longer exists in this Project.');
        await ctx.sessions.refresh();
        ctx.uiWorkspace.openSession(sessionId);
        ctx.layout.setSidebarOpen(false);
        reportSelection();
      }

      async function deliver(payload) {
        if (typeof payload.sessionId !== 'string' || typeof payload.requestId !== 'string' || typeof payload.text !== 'string' || !['queue', 'steer'].includes(payload.mode)) throw new Error('Invalid pending text.');
        if (dispatches.has(payload.requestId)) return;
        await persistence.dispatch({sessionId: payload.sessionId}, payload.text, payload.mode, payload.requestId, () => ctx.sessions.using(payload.sessionId, {source: 'devvmPendingText'}, async reference => {
          const binding = await reference.ready;
          return ctx.conversation.sendPendingText(binding.session, payload.text, payload.mode, payload.requestId);
        }));
      }

      async function receive(event) {
        const message = event.data;
        if (event.source !== parent || !config.controlOrigins.includes(event.origin) || !message || message.protocol !== 'devvm-embed' || message.version !== 1 || message.projectId !== config.projectId) return;
        if (message.kind === 'attach') {
          if (typeof message.channelId !== 'string' || attachment && attachment.channelId !== message.channelId) return;
          attachment = {channelId: message.channelId, origin: event.origin};
          post('ready', {connection: connectionState(), sessionId: activeSession});
          if (!persistenceRegistered) {
            persistenceRegistered = true;
            disposePersistence = ctx.conversation.setTextPersistence(persistence);
          }
          observeSelection();
          return;
        }
        if (!attachment || message.channelId !== attachment.channelId || message.instanceId !== instanceId) return;
        const {kind, payload, commandId} = message;
        if (kind === 'result') {
          const command = commands.get(commandId);
          if (!command || command.channelId !== attachment.channelId) return;
          clearTimeout(command.timeout);
          commands.delete(commandId);
          if (payload.error) command.reject(new Error(payload.error));
          else command.resolve(payload);
          return;
        }
        const run = async () => {
          try {
            if (kind === 'prepare-switch') { await flushDrafts(); ctx.layout.setSidebarOpen(false); }
            else if (kind === 'select-session') await selectSession(payload.sessionId);
            else if (kind === 'deliver') { await deliver(payload); return; }
            else if (kind === 'open-navigation') openNavigation();
            else if (kind === 'close-navigation') ctx.layout.setSidebarOpen(false);
            else if (kind === 'reauthorize') await reauthorize(payload.url);
            else if (kind === 'foreground') { if (connectionState() !== 'connected') ctx.connection.reconnect(); }
            else return;
            if (commandId) post('result', {}, commandId);
          } catch (error) {
            if (commandId) post('result', {error: error.message}, commandId);
            else post('error', {message: error.message});
          }
        };
        if (kind === 'deliver') run().catch(error => post('error', {message: error.message}));
        else sequence = sequence.then(run);
      }

      let persistenceRegistered = false;
      let disposePersistence;
      ctx.effect(() => {
        const onMessage = event => { receive(event).catch(error => post('error', {message: error.message})); };
        window.addEventListener('message', onMessage);
        let navigationTouch = null;
        const clearTouch = () => { navigationTouch = null; };
        const onTouchStart = event => {
          clearTouch();
          if (!matchMedia('(max-width: 1023px)').matches || event.touches.length !== 1) return;
          const touch = event.touches[0];
          navigationTouch = {id: touch.identifier, x: touch.clientX, y: touch.clientY};
        };
        const onTouchMove = event => {
          if (!navigationTouch) return;
          if (event.touches.length !== 1 || event.touches[0].identifier !== navigationTouch.id) { clearTouch(); return; }
          const touch = event.touches[0];
          const horizontal = touch.clientX - navigationTouch.x;
          const vertical = Math.abs(touch.clientY - navigationTouch.y);
          if (vertical > 16 && vertical >= Math.abs(horizontal)) { clearTouch(); return; }
          if (Math.abs(horizontal) <= vertical * 1.5) return;
          if (event.cancelable) event.preventDefault();
          if (horizontal >= 64) { clearTouch(); openNavigation(); }
          else if (horizontal <= -64) { clearTouch(); ctx.layout.setSidebarOpen(false); }
        };
        window.addEventListener('touchstart', onTouchStart, {passive: true, capture: true});
        window.addEventListener('touchmove', onTouchMove, {passive: false, capture: true});
        window.addEventListener('touchend', clearTouch, true);
        window.addEventListener('touchcancel', clearTouch, true);
        const disposeConnection = ctx.connection.state.subscribe(() => { reportConnection().catch(error => post('error', {message: error.message})); });
        const disposeGeneration = ctx.connection.generation.subscribe(() => {
          if (ctx.connection.generation.getSnapshot()) post('connection-changed', {state: 'connected'});
        });
        const authenticationTimer = setInterval(() => {
          if (connectionState() !== 'connected') reportConnection().catch(error => post('error', {message: error.message}));
        }, 5000);
        const disposeList = ctx.sessions.list.subscribe(observeSelection);
        observeSelection();
        for (const origin of config.controlOrigins) parent.postMessage({protocol: 'devvm-embed', version: 1, projectId: config.projectId, instanceId, kind: 'available'}, origin);
        return () => {
          disposed = true;
          window.removeEventListener('message', onMessage);
          window.removeEventListener('touchstart', onTouchStart, true);
          window.removeEventListener('touchmove', onTouchMove, true);
          window.removeEventListener('touchend', clearTouch, true);
          window.removeEventListener('touchcancel', clearTouch, true);
          clearInterval(authenticationTimer);
          disposeConnection();
          disposeGeneration();
          disposeList();
          disposePersistence?.();
          for (const dispose of mainObservers.values()) dispose();
          for (const command of commands.values()) { clearTimeout(command.timeout); command.reject(new Error('Project view closed.')); }
          commands.clear();
        };
      });
    }
    return {inject, apply};
  },
});
