// The Control origin owns drafts and pending text. A write is acknowledged only
// after its IndexedDB transaction commits, before the child clears or sends it.
const database = new Promise((resolve, reject) => {
  const request = indexedDB.open('devvm-text', 1);
  request.onupgradeneeded = () => {
    request.result.createObjectStore('drafts', {keyPath: ['daemon', 'projectId', 'sessionId']});
    request.result.createObjectStore('pending', {keyPath: ['daemon', 'projectId', 'sessionId', 'requestId']});
  };
  request.onerror = () => reject(request.error);
  request.onblocked = () => reject(new Error('Close the older DevVM app to update text storage.'));
  request.onsuccess = () => {
    request.result.onversionchange = () => request.result.close();
    resolve(request.result);
  };
});

async function transact(stores, mode, operation) {
  const db = await database;
  return new Promise((resolve, reject) => {
    const transaction = db.transaction(stores, mode, {durability: 'strict'});
    let result;
    transaction.oncomplete = () => resolve(result);
    transaction.onabort = () => reject(transaction.error || new Error('Text could not be saved.'));
    transaction.onerror = () => reject(transaction.error);
    operation(transaction, value => { result = value; });
  });
}

const draftKey = (projectId, sessionId) => [location.origin, projectId, sessionId];
const pendingKey = (projectId, sessionId, requestId) => [...draftKey(projectId, sessionId), requestId];

export function readDraft(projectId, sessionId) {
  return transact(['drafts'], 'readonly', (transaction, done) => {
    const request = transaction.objectStore('drafts').get(draftKey(projectId, sessionId));
    request.onsuccess = () => done(request.result ? {text: request.result.text, revision: request.result.revision} : {text: '', revision: 0});
  });
}

export function writeDraft(projectId, draft) {
  return transact(['drafts'], 'readwrite', (transaction, done) => {
    const store = transaction.objectStore('drafts');
    const request = store.get(draftKey(projectId, draft.sessionId));
    request.onsuccess = () => {
      const previous = request.result;
      if (previous && draft.revision < previous.revision) {
        transaction.abort();
        return;
      }
      store.put({...previous, daemon: location.origin, projectId, sessionId: draft.sessionId, text: draft.text, revision: draft.revision});
      done({revision: draft.revision});
    };
  });
}

export function admitText(projectId, prompt) {
  return transact(['drafts', 'pending'], 'readwrite', (transaction, done) => {
    const drafts = transaction.objectStore('drafts');
    const request = drafts.get(draftKey(projectId, prompt.sessionId));
    request.onsuccess = () => {
      const previous = request.result;
      if (previous && previous.revision !== prompt.draftRevision) {
        transaction.abort();
        return;
      }
      const order = (previous?.nextPromptOrder ?? 0) + 1;
      transaction.objectStore('pending').add({daemon: location.origin, projectId, sessionId: prompt.sessionId, requestId: prompt.requestId, text: prompt.text, mode: prompt.mode, order, state: 'waiting'});
      const revision = prompt.draftRevision + 1;
      drafts.put({daemon: location.origin, projectId, sessionId: prompt.sessionId, text: '', revision, nextPromptOrder: order});
      done({requestId: prompt.requestId, revision});
    };
  });
}

export function readPending() {
  return transact(['pending'], 'readonly', (transaction, done) => {
    const request = transaction.objectStore('pending').getAll();
    request.onsuccess = () => done(request.result.filter(record => record.daemon === location.origin).sort((a, b) => a.order - b.order));
  });
}

export function settleText(projectId, sessionId, requestId) {
  return transact(['pending'], 'readwrite', transaction => {
    transaction.objectStore('pending').delete(pendingKey(projectId, sessionId, requestId));
  });
}

export function blockText(projectId, sessionId, requestId, error) {
  return transact(['pending'], 'readwrite', transaction => {
    const store = transaction.objectStore('pending');
    const request = store.get(pendingKey(projectId, sessionId, requestId));
    request.onsuccess = () => {
      if (request.result) store.put({...request.result, state: 'blocked', error});
    };
  });
}

export function retryText(projectId, sessionId, requestId) {
  return transact(['pending'], 'readwrite', transaction => {
    const store = transaction.objectStore('pending');
    const request = store.get(pendingKey(projectId, sessionId, requestId));
    request.onsuccess = () => {
      if (request.result) {
        const {error, ...record} = request.result;
        store.put({...record, state: 'waiting'});
      }
    };
  });
}
