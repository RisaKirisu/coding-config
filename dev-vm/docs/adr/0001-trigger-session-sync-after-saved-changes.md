# Trigger Session Sync after saved changes

Portable DSH State remains in DSH's local file backends. Session Sync copies it to the Sync Store after DSH saves a completed turn or a relevant workspace or message-feedback change. This was chosen over a Postgres persistence adapter, shutdown hooks, and filesystem watching because DSH already reports when these changes are saved, local operation still works without the Sync Store, and DevVM does not need to replace DSH's persistence implementation. Version one relies on the Single Writer Rule and eventual rsync completion.
