/**
 * DevVM Remote Sync - DSH Web Client Overlay
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
        if (typeof require === 'function') {
          React = require('react');
        }
      } catch (e) {}
      if (!React && typeof window !== 'undefined' && window.React) {
        React = window.React;
      }

      const inject = ['slots'];

      function getIndicatorVisuals(status, isDirty) {
        if (status === 'synchronizing') {
          return {
            statusClass: 'synchronizing',
            text: '◌ Syncing...',
            style: {
              backgroundColor: 'rgba(56, 189, 248, 0.15)',
              color: '#38bdf8',
              border: '1px solid #38bdf8',
            },
          };
        }
        if (status === 'failed') {
          return {
            statusClass: 'failed',
            text: '✕ Sync Failed (Click to retry)',
            style: {
              backgroundColor: 'rgba(234, 179, 8, 0.15)',
              color: '#eab308',
              border: '1px solid #eab308',
            },
          };
        }
        if (status === 'degraded' || (status !== 'synchronized' && isDirty)) {
          return {
            statusClass: 'degraded',
            text: '▲ Degraded',
            style: {
              backgroundColor: 'rgba(234, 179, 8, 0.15)',
              color: '#eab308',
              border: '1px solid #eab308',
            },
          };
        }
        return {
          statusClass: 'synchronized',
          text: '● Synced',
          style: {
            backgroundColor: 'rgba(34, 197, 94, 0.15)',
            color: '#22c55e',
            border: '1px solid #22c55e',
          },
        };
      }

      function SyncIndicatorAction() {
        const [status, setStatus] = React.useState('synchronized');
        const [isDirty, setIsDirty] = React.useState(false);

        const fetchStatus = React.useCallback(async () => {
          try {
            const res = await fetch('/api/sync/status');
            if (res.ok) {
              const data = await res.json();
              if (data.status) setStatus(data.status);
              setIsDirty(Boolean(data.isDirty));
            }
          } catch (e) {}
        }, []);

        React.useEffect(() => {
          fetchStatus();
          const interval = setInterval(fetchStatus, 3000);
          return () => clearInterval(interval);
        }, [fetchStatus]);

        const handleClick = async () => {
          setStatus('synchronizing');
          try {
            const res = await fetch('/api/sync/trigger', {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
            });
            if (res.ok) {
              const data = await res.json();
              if (data.status) setStatus(data.status);
            } else {
              setStatus('failed');
            }
          } catch (e) {
            setStatus('failed');
          }
          fetchStatus();
        };

        const visuals = getIndicatorVisuals(status, isDirty);

        return React.createElement(
          'div',
          {
            id: 'devvm-sync-indicator',
            className: `devvm-sync-indicator devvm-sync-${visuals.statusClass}`,
            title: 'Remote Sync: Portable DSH State (Click to Sync)',
            onClick: handleClick,
            style: {
              display: 'inline-flex',
              alignItems: 'center',
              gap: '6px',
              padding: '4px 8px',
              borderRadius: '4px',
              fontSize: '12px',
              fontWeight: '600',
              cursor: 'pointer',
              userSelect: 'none',
              transition: 'all 0.2s ease',
              ...visuals.style,
            },
          },
          visuals.text,
        );
      }

      function mountDomIndicator() {
        if (typeof document === 'undefined') return;
        let indicator = document.getElementById('devvm-sync-indicator');
        if (!indicator) {
          indicator = document.createElement('div');
          indicator.id = 'devvm-sync-indicator';
          indicator.title = 'Remote Sync: Portable DSH State (Click to Sync)';
          indicator.style.display = 'inline-flex';
          indicator.style.alignItems = 'center';
          indicator.style.gap = '6px';
          indicator.style.padding = '4px 8px';
          indicator.style.borderRadius = '4px';
          indicator.style.fontSize = '12px';
          indicator.style.fontWeight = '600';
          indicator.style.cursor = 'pointer';
          indicator.style.userSelect = 'none';
          indicator.style.transition = 'all 0.2s ease';

          const updateDom = (status, isDirty) => {
            const visuals = getIndicatorVisuals(status, isDirty);
            indicator.className = `devvm-sync-indicator devvm-sync-${visuals.statusClass}`;
            indicator.style.display = 'inline-flex';
            indicator.style.backgroundColor = visuals.style.backgroundColor;
            indicator.style.color = visuals.style.color;
            indicator.style.border = visuals.style.border;
            indicator.textContent = visuals.text;
          };

          const pollStatus = async () => {
            try {
              const res = await fetch('/api/sync/status');
              if (res.ok) {
                const data = await res.json();
                updateDom(data.status || 'synchronized', data.isDirty);
              }
            } catch (e) {}
          };

          indicator.addEventListener('click', async () => {
            updateDom('synchronizing', false);
            try {
              const res = await fetch('/api/sync/trigger', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
              });
              if (res.ok) {
                const data = await res.json();
                updateDom(data.status || 'synchronized', data.isDirty);
              } else {
                updateDom('failed', true);
              }
            } catch (e) {
              updateDom('failed', true);
            }
          });

          const header = document.querySelector('header') || document.body;
          if (header) {
            header.appendChild(indicator);
          }
          updateDom('synchronized', false);
          pollStatus();
          setInterval(pollStatus, 3000);
        }
      }

      function apply(ctx) {
        if (ctx?.slots?.inject && React && typeof React.createElement === 'function') {
          ctx.slots.inject('conversation.session.header.actions', () => {
            return ctx.slots.register(
              {
                name: 'conversation.session.header.actions',
                id: 'remote-sync',
                order: 100,
              },
              SyncIndicatorAction,
            );
          });
        } else {
          mountDomIndicator();
        }
      }

      exports.apply = apply;
      exports.inject = inject;
      return module.exports;
    },
  });
}
