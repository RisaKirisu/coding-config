// Model-fallback settings page. The host serves this file to the browser and
// the client module loader registers the factory below; `require` resolves the
// shell's seed words, so the page renders with the host's own React.
window.__ModuleLoader__.load({
  id: '@devvm/dsh-model-fallback',
  factory: (require) => {
    const React = require('react')

    const TEXT = {
      zh: {
        tabLabel: '模型回退',
        title: '模型回退',
        subtitle: '主模型在重试耗尽后仍失败时，按顺序改用下一个候选模型继续当前轮次。',
        fallbacks: '回退模型列表',
        fallbacksHint: '按顺序尝试。已选模型的下一项先试。',
        addFallback: '添加回退模型',
        retriableErrors: '可重试错误',
        retriableErrorsHint: '匹配到的失败会按“重试为”的代码交给提供方重试策略，重试耗尽后再回退。',
        addError: '添加错误',
        provider: '提供方',
        model: '模型',
        effort: '思考强度',
        code: '错误代码',
        messageContains: '消息包含',
        treatAs: '重试为',
        anyMessage: '任意消息',
        defaultEffort: '默认',
        up: '上移',
        down: '下移',
        remove: '删除',
        save: '保存设置',
        saved: '设置已保存',
        loading: '正在加载模型列表…',
        emptyFallbacks: '尚未配置回退模型。',
        emptyErrors: '尚未配置可重试错误。',
        loadError: '加载失败：',
        saveError: '保存失败：',
      },
      en: {
        tabLabel: 'Model Fallback',
        title: 'Model Fallback',
        subtitle: 'When the primary model still fails after its retries are spent, continue the turn on the next candidate model.',
        fallbacks: 'Fallback models',
        fallbacksHint: 'Tried in order, starting after the model that failed.',
        addFallback: 'Add fallback model',
        retriableErrors: 'Retriable errors',
        retriableErrorsHint: 'A matched failure is retried by the provider policy under the "Treat as" code, and rolls to the next model once that budget is spent.',
        addError: 'Add error',
        provider: 'Provider',
        model: 'Model',
        effort: 'Reasoning effort',
        code: 'Error code',
        messageContains: 'Message contains',
        treatAs: 'Treat as',
        anyMessage: 'Any message',
        defaultEffort: 'Default',
        up: 'Move up',
        down: 'Move down',
        remove: 'Remove',
        save: 'Save settings',
        saved: 'Settings saved',
        loading: 'Loading models…',
        emptyFallbacks: 'No fallback models configured.',
        emptyErrors: 'No retriable errors configured.',
        loadError: 'Load failed: ',
        saveError: 'Save failed: ',
      },
    }

    const CSS = `
      .mfb-page{position:relative;width:100%;max-width:780px;color:var(--dsw-alias-label-primary,#1c1c1e);display:flex;flex-direction:column;gap:14px}
      .mfb-title{margin:0;font-size:19px;font-weight:680;line-height:26px}
      .mfb-subtitle{margin:0;font-size:13px;line-height:20px;color:var(--dsw-alias-label-secondary,#6d6d72)}
      .mfb-group{display:flex;flex-direction:column;gap:9px;padding:15px;border:1px solid var(--dsw-alias-border-l2,rgba(125,125,125,.3));border-radius:11px;background:var(--dsw-alias-bg-layer-2,rgba(125,125,125,.06))}
      .mfb-group-head{display:flex;align-items:baseline;justify-content:space-between;gap:12px}
      .mfb-group-title{margin:0;font-size:14px;font-weight:660}
      .mfb-hint{margin:0;font-size:12px;line-height:17px;color:var(--dsw-alias-label-tertiary,#8e8e93)}
      .mfb-row{display:flex;align-items:center;gap:7px}
      .mfb-row .mfb-grow{flex:1 1 0;min-width:0}
      .mfb-index{flex:0 0 20px;font-size:12px;text-align:right;color:var(--dsw-alias-label-tertiary,#8e8e93)}
      .mfb-head{margin-bottom:-2px}
      .mfb-col{font-size:12px;font-weight:600;color:var(--dsw-alias-label-tertiary,#8e8e93);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
      .mfb-spacer{flex:0 0 calc(3 * 28px + 2 * 7px)}
      .mfb-input,.mfb-select{width:100%;height:34px;padding:0 9px;box-sizing:border-box;border:1px solid var(--dsw-alias-border-l2,rgba(125,125,125,.3));border-radius:8px;background:var(--dsw-alias-bg-layer-1,rgba(125,125,125,.1));color:var(--dsw-alias-label-primary,#1c1c1e);font:inherit;font-size:13px;outline:none;color-scheme:light dark}
      .mfb-input:focus,.mfb-select:focus{border-color:#007aff;box-shadow:0 0 0 2px rgba(0,122,255,.22)}
      .mfb-icon{flex:0 0 auto;width:28px;height:28px;display:flex;align-items:center;justify-content:center;border:1px solid var(--dsw-alias-border-l2,rgba(125,125,125,.3));border-radius:7px;background:0 0;color:var(--dsw-alias-label-secondary,#6d6d72);font:inherit;font-size:13px;cursor:pointer}
      .mfb-icon:hover:not(:disabled){border-color:#007aff;color:#007aff}
      .mfb-icon:disabled{opacity:.35;cursor:default}
      .mfb-add{align-self:flex-start;height:30px;padding:0 13px;border:1px dashed var(--dsw-alias-border-l2,rgba(125,125,125,.5));border-radius:8px;background:0 0;color:var(--dsw-alias-label-secondary,#6d6d72);font:inherit;font-size:13px;cursor:pointer}
      .mfb-add:hover{border-color:#007aff;color:#007aff}
      .mfb-empty{margin:0;font-size:13px;color:var(--dsw-alias-label-tertiary,#8e8e93)}
      .mfb-actions{display:flex;align-items:center;gap:12px}
      .mfb-save{height:36px;padding:0 20px;border:0;border-radius:9px;background:#007aff;color:#fff;font:inherit;font-size:14px;font-weight:650;white-space:nowrap;cursor:pointer;box-shadow:0 1px 3px rgba(0,0,0,.2)}
      .mfb-save:hover:not(:disabled){background:#0066d6}
      .mfb-save:disabled{opacity:.55;cursor:default}
      .mfb-notice{font-size:13px;color:var(--dsw-alias-state-success-primary,#22a447)}
      .mfb-error{font-size:13px;color:var(--dsw-alias-state-error-primary,#d84040)}
    `

    function localeKey(locale) {
      const active = locale?.getLocale?.()?.active || locale?.getSnapshot?.()?.active || 'en'
      return String(active).startsWith('zh') ? 'zh' : 'en'
    }

    /** Replace one row of an ordered list without mutating the others. */
    function patchRow(rows, index, patch) {
      return rows.map((row, at) => (at === index ? { ...row, ...patch } : row))
    }

    /** Remove one row, or move it by `delta` within the list. */
    function moveRow(rows, index, delta) {
      const target = index + delta
      if (target < 0 || target >= rows.length) return rows
      const next = rows.slice()
      const [row] = next.splice(index, 1)
      next.splice(target, 0, row)
      return next
    }

    function RowControls(props) {
      return React.createElement(
        React.Fragment,
        null,
        React.createElement('button', {
          className: 'mfb-icon', type: 'button', title: props.t.up, 'aria-label': props.t.up,
          disabled: props.index === 0, onClick: () => props.onMove(-1),
        }, '↑'),
        React.createElement('button', {
          className: 'mfb-icon', type: 'button', title: props.t.down, 'aria-label': props.t.down,
          disabled: props.index === props.last, onClick: () => props.onMove(1),
        }, '↓'),
        React.createElement('button', {
          className: 'mfb-icon', type: 'button', title: props.t.remove, 'aria-label': props.t.remove,
          onClick: props.onRemove,
        }, '×'),
      )
    }

    /** Name each column once per group, aligned with the row controls below. */
    function ColumnHeaders(props) {
      return React.createElement(
        'div',
        { className: 'mfb-row mfb-head' },
        React.createElement('span', { className: 'mfb-index' }),
        ...props.labels.map((label) => React.createElement('span', { className: 'mfb-col mfb-grow', key: label }, label)),
        React.createElement('span', { className: 'mfb-spacer' }),
      )
    }

    function FallbackPage(props) {
      const locale = props.locale
      const [, rerender] = React.useState(0)
      const [state, setState] = React.useState({
        loading: true,
        saving: false,
        providers: [],
        modelsByProvider: {},
        reasoningByModel: {},
        fallbacks: [],
        retriableErrors: [],
        notice: '',
        error: '',
      })

      React.useEffect(() => locale?.subscribe?.(() => rerender((value) => value + 1)), [locale])
      const t = TEXT[localeKey(locale)]

      React.useEffect(() => {
        let active = true
        Promise.all([
          fetch('/api/model-fallback/models').then((response) => {
            if (!response.ok) throw new Error(String(response.status))
            return response.json()
          }),
          fetch('/api/model-fallback/config').then((response) => {
            if (!response.ok) throw new Error(String(response.status))
            return response.json()
          }),
        ]).then(([directory, config]) => {
          if (!active) return
          setState((current) => ({
            ...current,
            loading: false,
            providers: directory.providers || [],
            modelsByProvider: directory.modelsByProvider || {},
            reasoningByModel: directory.reasoningByModel || {},
            fallbacks: config.fallbacks || [],
            retriableErrors: config.retriableErrors || [],
          }))
        }).catch((error) => {
          if (active) setState((current) => ({ ...current, loading: false, error: t.loadError + error.message }))
        })
        return () => { active = false }
      }, [])

      const setFallbacks = (fallbacks) => setState((current) => ({ ...current, fallbacks, notice: '', error: '' }))
      const setErrors = (retriableErrors) => setState((current) => ({ ...current, retriableErrors, notice: '', error: '' }))

      const save = async () => {
        setState((current) => ({ ...current, saving: true, notice: '', error: '' }))
        try {
          const response = await fetch('/api/model-fallback/config', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ fallbacks: state.fallbacks, retriableErrors: state.retriableErrors }),
          })
          if (!response.ok) {
            const failure = await response.json()
            throw new Error(failure.error || String(response.status))
          }
          // Render what persisted rather than what was submitted, so a
          // normalized or refused field cannot be shown as accepted.
          const stored = await fetch('/api/model-fallback/config')
          if (!stored.ok) throw new Error(String(stored.status))
          const value = await stored.json()
          setState((current) => ({
            ...current,
            saving: false,
            fallbacks: value.fallbacks || [],
            retriableErrors: value.retriableErrors || [],
            notice: t.saved,
          }))
          setTimeout(() => setState((current) => ({ ...current, notice: '' })), 3000)
        } catch (error) {
          setState((current) => ({ ...current, saving: false, error: t.saveError + error.message }))
        }
      }

      if (state.loading) {
        return React.createElement('div', { className: 'mfb-page' }, React.createElement('style', null, CSS), t.loading)
      }

      const providerOptions = state.providers.map((provider) => React.createElement(
        'option',
        { key: provider.id, value: provider.id },
        provider.name && provider.name !== provider.id ? provider.name + ' (' + provider.id + ')' : provider.id,
      ))

      const effortOptionsFor = (provider, model) => {
        const efforts = state.reasoningByModel[provider + '/' + model]
          || ['off', 'low', 'medium', 'high', 'xhigh', 'max']
        return [
          React.createElement('option', { key: '', value: '' }, t.defaultEffort),
          ...efforts.map((effort) => React.createElement('option', { key: effort, value: effort }, effort)),
        ]
      }

      const fallbackRows = state.fallbacks.map((row, index) => {
        const models = state.modelsByProvider[row.provider] || []
        const modelOptions = [
          React.createElement('option', { key: '', value: '' }, '—'),
          ...models.map((model) => React.createElement(
            'option',
            { key: model.id, value: model.id },
            model.name && model.name !== model.id ? model.name + ' (' + model.id + ')' : model.id,
          )),
        ]
        return React.createElement(
          'div',
          { className: 'mfb-row', key: 'f' + index },
          React.createElement('span', { className: 'mfb-index' }, String(index + 1)),
          React.createElement('select', {
            className: 'mfb-select mfb-grow',
            value: row.provider,
            'aria-label': t.provider,
            onChange: (event) => {
              const provider = event.target.value
              const next = state.modelsByProvider[provider] || []
              setFallbacks(patchRow(state.fallbacks, index, {
                provider,
                model: next[0]?.id || '',
                reasoningEffort: '',
              }))
            },
          }, React.createElement('option', { key: '', value: '' }, '—'), ...providerOptions),
          React.createElement('select', {
            className: 'mfb-select mfb-grow',
            value: row.model,
            'aria-label': t.model,
            onChange: (event) => setFallbacks(patchRow(state.fallbacks, index, { model: event.target.value, reasoningEffort: '' })),
          }, ...modelOptions),
          React.createElement('select', {
            className: 'mfb-select mfb-grow',
            value: row.reasoningEffort || '',
            'aria-label': t.effort,
            onChange: (event) => setFallbacks(patchRow(state.fallbacks, index, { reasoningEffort: event.target.value })),
          }, ...effortOptionsFor(row.provider, row.model)),
          React.createElement(RowControls, {
            t, index, last: state.fallbacks.length - 1,
            onMove: (delta) => setFallbacks(moveRow(state.fallbacks, index, delta)),
            onRemove: () => setFallbacks(state.fallbacks.filter((_, at) => at !== index)),
          }),
        )
      })

      const errorRows = state.retriableErrors.map((rule, index) => React.createElement(
        'div',
        { className: 'mfb-row', key: 'e' + index },
        React.createElement('span', { className: 'mfb-index' }, String(index + 1)),
        React.createElement('input', {
          className: 'mfb-input mfb-grow', value: rule.code, placeholder: t.code, 'aria-label': t.code,
          onChange: (event) => setErrors(patchRow(state.retriableErrors, index, { code: event.target.value })),
        }),
        React.createElement('input', {
          className: 'mfb-input mfb-grow', value: rule.messageContains, placeholder: t.anyMessage,
          'aria-label': t.messageContains,
          onChange: (event) => setErrors(patchRow(state.retriableErrors, index, { messageContains: event.target.value })),
        }),
        React.createElement('input', {
          className: 'mfb-input mfb-grow', value: rule.treatAs, placeholder: 'TRANSPORT', 'aria-label': t.treatAs,
          onChange: (event) => setErrors(patchRow(state.retriableErrors, index, { treatAs: event.target.value })),
        }),
        React.createElement(RowControls, {
          t, index, last: state.retriableErrors.length - 1,
          onMove: (delta) => setErrors(moveRow(state.retriableErrors, index, delta)),
          onRemove: () => setErrors(state.retriableErrors.filter((_, at) => at !== index)),
        }),
      ))

      return React.createElement(
        'div',
        { className: 'mfb-page' },
        React.createElement('style', null, CSS),
        React.createElement('h2', { className: 'mfb-title' }, t.title),
        React.createElement('p', { className: 'mfb-subtitle' }, t.subtitle),
        React.createElement(
          'div',
          { className: 'mfb-group' },
          React.createElement(
            'div',
            { className: 'mfb-group-head' },
            React.createElement('h3', { className: 'mfb-group-title' }, t.fallbacks),
          ),
          React.createElement('p', { className: 'mfb-hint' }, t.fallbacksHint),
          fallbackRows.length === 0
            ? React.createElement('p', { className: 'mfb-empty' }, t.emptyFallbacks)
            : React.createElement(ColumnHeaders, { labels: [t.provider, t.model, t.effort] }),
          ...fallbackRows,
          React.createElement('button', {
            className: 'mfb-add', type: 'button',
            onClick: () => setFallbacks([
              ...state.fallbacks,
              { provider: state.providers[0]?.id || '', model: '', reasoningEffort: '' },
            ]),
          }, '+ ' + t.addFallback),
        ),
        React.createElement(
          'div',
          { className: 'mfb-group' },
          React.createElement('h3', { className: 'mfb-group-title' }, t.retriableErrors),
          React.createElement('p', { className: 'mfb-hint' }, t.retriableErrorsHint),
          errorRows.length === 0
            ? React.createElement('p', { className: 'mfb-empty' }, t.emptyErrors)
            : React.createElement(ColumnHeaders, { labels: [t.code, t.messageContains, t.treatAs] }),
          ...errorRows,
          React.createElement('button', {
            className: 'mfb-add', type: 'button',
            onClick: () => setErrors([
              ...state.retriableErrors,
              { code: '', messageContains: '', treatAs: 'TRANSPORT' },
            ]),
          }, '+ ' + t.addError),
        ),
        React.createElement(
          'div',
          { className: 'mfb-actions' },
          React.createElement('button', {
            className: 'mfb-save', type: 'button', disabled: state.saving, onClick: save,
          }, t.save),
          state.notice ? React.createElement('span', { className: 'mfb-notice' }, state.notice) : null,
          state.error ? React.createElement('span', { className: 'mfb-error' }, state.error) : null,
        ),
      )
    }

    return {
      name: '@devvm/dsh-model-fallback',
      inject: ['slots', 'locale'],
      apply(ctx) {
        ctx.slots.inject('settings.section', () => ctx.slots.register(
          {
            name: 'settings.section',
            id: 'model-fallback',
            order: 16,
            label: () => TEXT[localeKey(ctx.locale)].tabLabel,
          },
          () => React.createElement(FallbackPage, { locale: ctx.locale }),
        ))
      },
    }
  },
})
