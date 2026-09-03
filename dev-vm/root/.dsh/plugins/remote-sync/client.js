/**
 * DevVM Remote Sync - DSH Web client indicator and Sync Store banner.
 */
if (typeof window !== 'undefined') {
  window.__ModuleLoader__ = window.__ModuleLoader__ || {
    load: (mod) => {
      if (typeof window.__registerDshPlugin === 'function') {
        window.__registerDshPlugin(mod);
      }
    },
  };

  window.__ModuleLoader__.load({
    id: '@devvm/dsh-remote-sync',
    factory: (require) => {
      var module = { exports: {} };
      var exports = module.exports;

      let React;
      try {
        if (typeof require === 'function') React = require('react');
      } catch (error) {
        console.warn('remote-sync: React is unavailable, no UI registered', error);
      }

      const inject = ['slots'];

      const POLL_INTERVAL_MS = 3000;
      const CHECK_INTERVAL_MS = 30000;
      const STYLE_ID = 'devvm-sync-style';
      const PULSE_CSS =
        '@keyframes devvm-sync-pulse{0%{opacity:1}50%{opacity:0.45}100%{opacity:1}}' +
        '.devvm-sync-synchronizing{animation:devvm-sync-pulse 1.2s ease-in-out infinite}';

      function injectStyleOnce() {
        if (typeof document === 'undefined') return;
        if (document.getElementById(STYLE_ID)) return;
        const tag = document.createElement('style');
        tag.id = STYLE_ID;
        tag.textContent = PULSE_CSS;
        document.head.appendChild(tag);
      }

      const YELLOW = { backgroundColor: 'rgba(234, 179, 8, 0.15)', color: '#eab308', border: '1px solid #eab308' };

      const VISUALS = {
        synchronized: {
          text: '● Synced',
          style: { backgroundColor: 'rgba(34, 197, 94, 0.15)', color: '#22c55e', border: '1px solid #22c55e' },
        },
        not_configured: {
          text: '○ Sync off',
          style: { backgroundColor: 'rgba(148, 163, 184, 0.15)', color: '#94a3b8', border: '1px solid #94a3b8' },
        },
        synchronizing: {
          text: '◌ Syncing…',
          style: { backgroundColor: 'rgba(56, 189, 248, 0.15)', color: '#38bdf8', border: '1px solid #38bdf8' },
        },
        failed: { text: '✕ Sync failed — click to retry', style: YELLOW },
        degraded: { text: '▲ Sync Store unreachable — click to retry', style: YELLOW },
        remote_ahead: { text: '▲ Sync Store ahead — restart DSH', style: YELLOW },
      };

      const CLICKABLE = { failed: true, degraded: true, remote_ahead: true };

      /**
       * One shared poller: both the header indicator and the overlay banner read
       * the same Sync Status, and the focus-time Sync Store check is debounced
       * across them.
       */
      function createStatusStore() {
        let state = null;
        let timer = null;
        let lastCheckAt = 0;
        const listeners = new Set();

        function publish(next) {
          state = next;
          for (const listener of listeners) listener();
        }

        async function request(path, options) {
          try {
            const res = await fetch(path, options);
            if (!res.ok) {
              console.warn(`remote-sync: ${path} responded ${res.status}`);
              return;
            }
            publish(await res.json());
          } catch (error) {
            console.warn(`remote-sync: ${path} request failed`, error);
          }
        }

        const refresh = () => request('/api/sync/status');
        const post = (path) =>
          request(path, { method: 'POST', headers: { 'Content-Type': 'application/json' } });

        function checkRemote() {
          const now = Date.now();
          if (now - lastCheckAt < CHECK_INTERVAL_MS) return;
          lastCheckAt = now;
          post('/api/sync/check');
        }

        function onVisibilityChange() {
          if (document.visibilityState === 'visible') checkRemote();
        }

        function start() {
          timer = setInterval(refresh, POLL_INTERVAL_MS);
          window.addEventListener('focus', checkRemote);
          document.addEventListener('visibilitychange', onVisibilityChange);
          refresh();
        }

        function stop() {
          clearInterval(timer);
          timer = null;
          window.removeEventListener('focus', checkRemote);
          document.removeEventListener('visibilitychange', onVisibilityChange);
        }

        return {
          get: () => state,
          retry: () => post('/api/sync/retry'),
          subscribe(listener) {
            listeners.add(listener);
            if (listeners.size === 1) start();
            return () => {
              listeners.delete(listener);
              if (listeners.size === 0) stop();
            };
          },
        };
      }

      const store = createStatusStore();

      function useSyncStatus() {
        const [snapshot, setSnapshot] = React.useState(store.get());
        React.useEffect(() => store.subscribe(() => setSnapshot(store.get())), []);
        return snapshot;
      }

      function SyncIndicatorAction() {
        const snapshot = useSyncStatus();
        React.useEffect(injectStyleOnce, []);
        if (!snapshot) return null;

        const status = snapshot.status;
        const visuals = VISUALS[status];
        if (!visuals) return null;
        const clickable = Boolean(CLICKABLE[status]);

        return React.createElement(
          'div',
          {
            id: 'devvm-sync-indicator',
            className: `devvm-sync-indicator devvm-sync-${status}`,
            title: snapshot.last_error || 'Session Sync of Portable DSH State',
            onClick: clickable ? () => store.retry() : undefined,
            style: {
              display: 'inline-flex',
              alignItems: 'center',
              gap: '6px',
              padding: '4px 8px',
              borderRadius: '4px',
              fontSize: '12px',
              fontWeight: '600',
              cursor: clickable ? 'pointer' : 'default',
              userSelect: 'none',
              ...visuals.style,
            },
          },
          visuals.text,
        );
      }

      function SyncStoreBanner() {
        const snapshot = useSyncStatus();
        if (!snapshot || snapshot.status !== 'remote_ahead') return null;

        const link =
          snapshot.daemon_url && snapshot.project_id
            ? `${snapshot.daemon_url}/#card-${snapshot.project_id}`
            : null;

        return React.createElement(
          'div',
          {
            className: 'devvm-sync-banner',
            style: {
              position: 'fixed',
              top: 0,
              left: 0,
              right: 0,
              zIndex: 1000,
              // Only the banner itself takes pointer events; the app stays usable.
              pointerEvents: 'auto',
              display: 'flex',
              justifyContent: 'center',
              gap: '8px',
              padding: '8px 12px',
              fontSize: '13px',
              fontWeight: '600',
              backgroundColor: '#eab308',
              color: '#1f2937',
            },
          },
          "Another workstation has written to this Project's Sync Store. Restart DSH from the DevVM page to load it.",
          link
            ? React.createElement(
                'a',
                { href: link, style: { color: '#1f2937', textDecoration: 'underline' } },
                'Open the DevVM page',
              )
            : null,
        );
      }

      function apply(ctx) {
        if (!React || typeof React.createElement !== 'function') return;
        if (!ctx?.slots?.inject) return;

        ctx.slots.inject('conversation.session.header.actions', () =>
          ctx.slots.register(
            { name: 'conversation.session.header.actions', id: 'remote-sync', order: 100 },
            SyncIndicatorAction,
          ),
        );

        ctx.slots.inject('shell.overlay', () =>
          ctx.slots.register(
            { name: 'shell.overlay', id: 'remote-sync-banner', order: 100 },
            SyncStoreBanner,
          ),
        );
      }

      exports.apply = apply;
      exports.inject = inject;
      return module.exports;
    },
  });
}
