window.__ModuleLoader__.load({
  id: '@devvm/dsh-subagent-manager',
  factory: (require) => {
    var module = { exports: {} };
    var exports = module.exports;
    const React = require('react');

    const LOCALE_NS = 'settings.subagentManager';
    const TEXT = {
      zh: {
        tabLabel: '子智能体模型',
        title: '子智能体模型与思考强度配置',
        subtitle: '为子智能体设置独立模型和思考强度。留空则继承主会话配置。',
        provider: '模型供应商',
        model: '模型',
        effort: '思考强度档位',
        inheritProvider: '-- 继承主会话供应商 --',
        selectModel: '-- 选择模型 --',
        inheritEffort: '-- 默认 / 继承 --',
        save: '保存设置',
        saved: '设置已保存',
        loading: '正在加载模型列表…',
        active: '当前生效规则：',
        inherited: '未单独配置，子智能体将继承主会话模型',
        custom: '独立模型：',
        loadError: '加载失败：',
        saveError: '保存失败：',
      },
      en: {
        tabLabel: 'Subagent Model',
        title: 'Subagent Model and Reasoning Effort',
        subtitle: 'Choose a dedicated model and reasoning effort for subagents. Leave blank to inherit the main session.',
        provider: 'Provider',
        model: 'Model',
        effort: 'Reasoning Effort',
        inheritProvider: '-- Inherit from Main Session --',
        selectModel: '-- Select Model --',
        inheritEffort: '-- Default / Inherit --',
        save: 'Save Settings',
        saved: 'Settings saved',
        loading: 'Loading models…',
        active: 'Active rule:',
        inherited: 'Unconfigured — subagents inherit the main session model',
        custom: 'Dedicated model:',
        loadError: 'Load failed: ',
        saveError: 'Save failed: ',
      },
    };

    const CSS = `
      .sam-page{padding:24px 32px;max-width:720px;color:var(--dsw-alias-label-primary,#1c1c1e);font-family:system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;box-sizing:border-box}
      .sam-title{font-size:20px;font-weight:700;margin:0 0 6px;color:var(--dsw-alias-label-primary,#1c1c1e)}
      .sam-subtitle{font-size:14px;line-height:1.55;margin:0 0 22px;color:var(--dsw-alias-label-secondary,#6d6d72)}
      .sam-card{padding:20px;border:1px solid var(--dsw-alias-border-l1,rgba(125,125,125,.2));border-radius:14px;background:var(--dsw-alias-bg-layer-1,rgba(125,125,125,.07))}
      .sam-field{margin-bottom:17px}.sam-field:last-child{margin-bottom:0}
      .sam-label{display:block;margin:0 0 7px;font-size:13px;font-weight:650;color:var(--dsw-alias-label-primary,#1c1c1e)}
      .sam-select-wrap{position:relative}
      .sam-select{width:100%;height:42px;padding:0 38px 0 13px;appearance:none;-webkit-appearance:none;border:1px solid var(--dsw-alias-border-l2,rgba(125,125,125,.3));border-radius:9px;background:var(--dsw-alias-bg-layer-2,rgba(125,125,125,.1));color:var(--dsw-alias-label-primary,#1c1c1e);font-size:14px;outline:none;box-sizing:border-box;color-scheme:light dark}
      .sam-select:focus{border-color:#007aff;box-shadow:0 0 0 2px rgba(0,122,255,.22)}
      .sam-arrow{position:absolute;right:13px;top:50%;transform:translateY(-50%);pointer-events:none;color:var(--dsw-alias-label-secondary,#6d6d72)}
      .sam-summary{margin-top:18px;padding:13px 15px;border:1px solid rgba(0,122,255,.24);border-radius:10px;background:rgba(0,122,255,.08);font-size:13px;line-height:1.5}
      .sam-actions{display:flex;align-items:center;gap:12px;margin-top:20px}
      .sam-save{height:38px;padding:0 22px;border:0;border-radius:9px;background:#007aff;color:#fff;font-size:14px;font-weight:650;white-space:nowrap;cursor:pointer;box-shadow:0 1px 3px rgba(0,0,0,.2)}
      .sam-save:hover{background:#0066d6}.sam-save:disabled{opacity:.55;cursor:default}
      .sam-notice{font-size:13px;color:var(--dsw-alias-state-success-primary,#22a447)}
      .sam-error{font-size:13px;color:var(--dsw-alias-state-error-primary,#d84040)}
    `;

    function localeKey(locale) {
      const active = locale?.getLocale?.()?.active || locale?.getSnapshot?.()?.active || 'en';
      return String(active).startsWith('zh') ? 'zh' : 'en';
    }

    function SelectField(props) {
      return React.createElement(
        'div',
        { className: 'sam-field' },
        React.createElement('label', { className: 'sam-label' }, props.label),
        React.createElement(
          'div',
          { className: 'sam-select-wrap' },
          React.createElement('select', {
            className: 'sam-select',
            value: props.value,
            onChange: props.onChange,
          }, ...props.options),
          React.createElement('span', { className: 'sam-arrow' }, '⌄'),
        ),
      );
    }

    function SettingsPage(props) {
      const locale = props.locale;
      const [, rerender] = React.useState(0);
      const [state, setState] = React.useState({
        loading: true,
        saving: false,
        providers: [],
        modelsByProvider: {},
        reasoningByModel: {},
        provider: '',
        model: '',
        effort: '',
        notice: '',
        error: '',
      });

      React.useEffect(() => locale?.subscribe?.(() => rerender((value) => value + 1)), [locale]);
      const t = TEXT[localeKey(locale)];

      React.useEffect(() => {
        let active = true;
        Promise.all([
          fetch('/api/subagent-manager/models').then((response) => {
            if (!response.ok) throw new Error(String(response.status));
            return response.json();
          }),
          fetch('/api/subagent-manager/config').then((response) => {
            if (!response.ok) throw new Error(String(response.status));
            return response.json();
          }),
        ]).then(([directory, config]) => {
          if (!active) return;
          setState((current) => ({
            ...current,
            loading: false,
            providers: directory.providers || [],
            modelsByProvider: directory.modelsByProvider || {},
            reasoningByModel: directory.reasoningByModel || {},
            provider: config.provider || '',
            model: config.model || '',
            effort: config.reasoningEffort || '',
          }));
        }).catch((error) => {
          if (active) setState((current) => ({ ...current, loading: false, error: t.loadError + error.message }));
        });
        return () => { active = false; };
      }, []);

      const setProvider = (event) => {
        const provider = event.target.value;
        const models = state.modelsByProvider[provider] || [];
        setState((current) => ({
          ...current,
          provider,
          model: models[0]?.id || '',
          effort: '',
          notice: '',
          error: '',
        }));
      };

      const setModel = (event) => setState((current) => ({
        ...current,
        model: event.target.value,
        effort: '',
        notice: '',
        error: '',
      }));

      const save = async () => {
        setState((current) => ({ ...current, saving: true, notice: '', error: '' }));
        try {
          const response = await fetch('/api/subagent-manager/config', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              provider: state.provider,
              model: state.model,
              reasoningEffort: state.effort,
            }),
          });
          const value = await response.json();
          if (!response.ok) throw new Error(value.error || String(response.status));
          setState((current) => ({
            ...current,
            saving: false,
            provider: value.provider || '',
            model: value.model || '',
            effort: value.reasoningEffort || '',
            notice: t.saved,
          }));
          setTimeout(() => setState((current) => ({ ...current, notice: '' })), 3000);
        } catch (error) {
          setState((current) => ({ ...current, saving: false, error: t.saveError + error.message }));
        }
      };

      if (state.loading) {
        return React.createElement('div', { className: 'sam-page' }, React.createElement('style', null, CSS), t.loading);
      }

      const models = state.provider ? (state.modelsByProvider[state.provider] || []) : [];
      const efforts = state.reasoningByModel[state.provider + '/' + state.model]
        || ['off', 'low', 'medium', 'high', 'xhigh', 'max'];

      const providerOptions = [
        React.createElement('option', { key: '', value: '' }, t.inheritProvider),
        ...state.providers.map((provider) => React.createElement(
          'option',
          { key: provider.id, value: provider.id },
          provider.name && provider.name !== provider.id
            ? provider.name + ' (' + provider.id + ')'
            : provider.id,
        )),
      ];
      const modelOptions = [
        React.createElement('option', { key: '', value: '' }, t.selectModel),
        ...models.map((model) => React.createElement(
          'option',
          { key: model.id, value: model.id },
          model.name && model.name !== model.id ? model.name + ' (' + model.id + ')' : model.id,
        )),
      ];
      const effortOptions = [
        React.createElement('option', { key: '', value: '' }, t.inheritEffort),
        ...efforts.map((effort) => React.createElement('option', { key: effort, value: effort }, effort)),
      ];

      return React.createElement(
        'div',
        { className: 'sam-page' },
        React.createElement('style', null, CSS),
        React.createElement('h2', { className: 'sam-title' }, t.title),
        React.createElement('p', { className: 'sam-subtitle' }, t.subtitle),
        React.createElement(
          'div',
          { className: 'sam-card' },
          React.createElement(SelectField, { label: t.provider, value: state.provider, onChange: setProvider, options: providerOptions }),
          state.provider
            ? React.createElement(SelectField, { label: t.model, value: state.model, onChange: setModel, options: modelOptions })
            : null,
          state.provider && state.model
            ? React.createElement(SelectField, {
                label: t.effort,
                value: state.effort,
                onChange: (event) => setState((current) => ({ ...current, effort: event.target.value, notice: '', error: '' })),
                options: effortOptions,
              })
            : null,
        ),
        React.createElement(
          'div',
          { className: 'sam-summary' },
          React.createElement('strong', null, t.active + ' '),
          state.provider && state.model
            ? t.custom + ' ' + state.provider + ' / ' + state.model + (state.effort ? ' (' + state.effort + ')' : '')
            : t.inherited,
        ),
        React.createElement(
          'div',
          { className: 'sam-actions' },
          React.createElement('button', { className: 'sam-save', type: 'button', disabled: state.saving, onClick: save }, t.save),
          state.notice ? React.createElement('span', { className: 'sam-notice' }, state.notice) : null,
          state.error ? React.createElement('span', { className: 'sam-error' }, state.error) : null,
        ),
      );
    }

    module.exports = {
      name: '@devvm/dsh-subagent-manager',
      inject: ['slots', 'locale'],
      apply(ctx) {
        const slots = ctx.get('slots');
        const locale = ctx.get('locale');
        if (!slots || !locale) return;

        slots.inject('settings.section', () => slots.register(
          {
            name: 'settings.section',
            id: 'subagent-model-config',
            order: 14,
            label: () => TEXT[localeKey(locale)].tabLabel,
          },
          () => React.createElement(SettingsPage, { locale }),
        ));
      },
    };

    return module.exports;
  },
});
