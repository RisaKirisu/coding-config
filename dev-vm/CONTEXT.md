# DevVM Workspace Supervision

This context defines the language for managing isolated project workspaces, their coding-agent runtimes, and portable agent history.

## Language

**Project**:
A host directory registered as one isolated development workspace and identified by a committed Project ID.
_Avoid_: Repository, workspace, folder

**Project ID**:
A stable UUID stored in `.devvm-id` that identifies a Project independently of its path on any workstation.
_Avoid_: Project hash, path hash, machine ID

**DevVM**:
The isolated microVM assigned to one Project. A Project has at most one active DevVM on a workstation.
_Avoid_: Container, sandbox, workspace VM

**DSH Runtime**:
The managed DeepSeek Harness process running inside a DevVM. Its lifecycle and status are distinct from the DevVM lifecycle.
_Avoid_: DSH server, Harness VM

**Portable DSH State**:
Project-specific DSH history and user state that follows a Project between workstations. It excludes rebuildable caches and workstation-wide configuration.
_Avoid_: Project `.dsh`, session folder, artifacts

**Sync Store**:
The VPS-hosted canonical directory for each Project's Portable DSH State.
_Avoid_: Cloud, session server, remote database

**Session Sync**:
Synchronization of a Project's DSH sessions and related Portable DSH State between its DevVM and Sync Store.
_Avoid_: Specialized architecture jargon for ordinary session syncing

**Dirty Local State**:
Portable DSH State containing durable local changes not yet confirmed in the Sync Store. Dirty Local State takes precedence during startup reconciliation.
_Avoid_: Unsaved state, pending cache

**Loopback Facade**:
The ingress behavior that presents routed requests to applications as loopback-origin traffic while preserving the browser-facing Project URL.
_Avoid_: Trusted host, localhost patch, proxy rewrite

**Project Browser**:
The host-side directory view used to register Projects, limited to directories beneath the daemon user's home.
_Avoid_: File picker, workspace picker

**Project URL**:
A browser address for an application inside a DevVM, available through local loopback naming or private tailnet naming.
_Avoid_: Port mapping, tunnel URL

**Degraded Sync**:
The state in which a Project continues using local Portable DSH State while its Sync Store is unavailable.
_Avoid_: Offline mode, sync disabled

**Control Daemon**:
The host process that manages Projects, DevVMs, and DSH Runtimes. It may run interactively or under the host's user-level service manager.
_Avoid_: Supervisor, backend, manager service

**Single Writer Rule**:
The operating rule that only one workstation may actively modify a Project's Portable DSH State at a time. Version one relies on user discipline rather than detecting or enforcing concurrent writers.
_Avoid_: User lock, chat lock

**Tailnet Boundary**:
The access boundary in which Tailscale membership and ACLs authorize remote use without an additional application login.
_Avoid_: Public access, local network access

**Unregister**:
Remove a Project from the Control Daemon's registry without deleting its DevVM or Sync Store data.
_Avoid_: Delete project, remove VM

**Sync Status**:
The latest observable state of a Project's synchronization: not yet synchronized, synchronizing, synchronized, or failed.
_Avoid_: Connection status, backup status

**Project Log**:
A host-persisted diagnostic stream associated with one Project and one managed process or operation.
_Avoid_: VM console, terminal history
