import { apiFetch } from './api.js';

const request = async (path, method, body) => {
    const response = await apiFetch(path, {
        method,
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    const data = await response.json().catch(() => null);
    if (!response.ok) {
        throw new Error(data?.error || `${method} ${path} failed (${response.status})`);
    }
    if (method !== 'DELETE' && data === null) {
        throw new Error(`${method} ${path} returned an invalid response`);
    }
    return data;
};

export const createResourceApi = (path) => ({
    list: () => request(path, 'GET'),
    create: (item) => saveItem(path, 'POST', item),
    update: (item) => saveItem(path, 'PUT', item),
    remove: (id) => request(`${path}/${encodeURIComponent(id)}`, 'DELETE'),
});

const saveItem = async (path, method, item) => {
    const saved = await request(path, method, item);
    if (!saved || saved.id !== item.id) throw new Error(`${method} ${path} returned an invalid resource`);
    return saved;
};

// Report each successful operation so a partial save can be retried safely.
export const saveResourceChanges = async (api, original, draft, onSaved) => {
    const originalById = new Map(original.map((item) => [item.id, item]));
    const draftById = new Map(draft.map((item) => [item.id, item]));

    for (const item of original) {
        if (!draftById.has(item.id)) {
            await api.remove(item.id);
            onSaved('remove', item);
        }
    }
    for (const item of draft) {
        const previous = originalById.get(item.id);
        if (!previous) {
            onSaved('create', await api.create(item));
        } else if (JSON.stringify(previous) !== JSON.stringify(item)) {
            onSaved('update', await api.update(item));
        }
    }
};
