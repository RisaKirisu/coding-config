---
name: rpi5-openwrt-ssh-sftp
description: Connect to the Raspberry Pi 5 OpenWrt router at 192.168.114.1 over SSH or SFTP through Paramiko from the approved scratchpad. Use when the assistant needs ssh connection to the RPI5 to complete an user request.
---

# RPi5 OpenWrt SSH/SFTP

Use this workflow for assistant-side SSH and SFTP connections to the Raspberry Pi 5 OpenWrt router.

## Target

- Host: `192.168.114.1`
- SSH user: `root`
- SSH port: `22`
- Platform: OpenWrt on Raspberry Pi 5

## Safety Rules

- Do not store the router password in files, docs, scripts, or committed config.
- Use the password only when the user explicitly authorizes it for the current task/session.
- Keep local scripts, downloaded files, and intermediate output under `/home/risa/chat_agent_scratchpad/` only.
- Run all Python from `/home/risa/chat_agent_scratchpad` using `uv run python`.
- Do not use noninteractive `ssh root@192.168.114.1`; this environment cannot answer password prompts and `sshpass` is unavailable.
- Prefer read-only OpenWrt commands unless the user explicitly authorizes a change.
- Before changing OpenWrt config or service state, inspect current state, state exactly what will change, and request user approval.

## OpenWrt OS Checks

Generic OS-level checks are in scope for this skill. Examples: `uname -a`, `uptime`, `logread`, `dmesg`, `ifconfig`, `ip addr`, `ubus`, and `/etc/init.d/<service> status`.

## SSH Command Pattern

Run with the Bash tool using `workdir: /home/risa/chat_agent_scratchpad`. Provide `RPI5_ROOT_PASSWORD` only temporarily for that command/session.

- Use Paramiko, not `ssh`.
- Set `cmd` to the smallest read-only probe that answers the question.

```sh
uv run python - <<'PY'
import os
import paramiko

host = '192.168.114.1'
user = 'root'
password = os.environ['RPI5_ROOT_PASSWORD']

cmd = 'uname -a; uptime'

client = paramiko.SSHClient()
client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
client.connect(
    hostname=host,
    username=user,
    password=password,
    timeout=10,
    banner_timeout=10,
    auth_timeout=10,
)

stdin, stdout, stderr = client.exec_command(cmd, timeout=30)
print(stdout.read().decode(errors='replace'))
err = stderr.read().decode(errors='replace')
if err:
    print('STDERR:', err)

client.close()
PY
```

## SFTP Download Pattern

Use SFTP for file transfers. Keep local paths under `/home/risa/chat_agent_scratchpad/`.

- Create any local destination directory under `/home/risa/chat_agent_scratchpad/` before downloading.

```sh
uv run python - <<'PY'
import os
import paramiko

host = '192.168.114.1'
user = 'root'
password = os.environ['RPI5_ROOT_PASSWORD']

remote_path = '/path/on/router/file'
local_path = '/home/risa/chat_agent_scratchpad/rpi5_openwrt_files/file'

client = paramiko.SSHClient()
client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
client.connect(hostname=host, username=user, password=password, timeout=10, banner_timeout=10, auth_timeout=10)

sftp = client.open_sftp()
sftp.get(remote_path, local_path)
sftp.close()
client.close()

print(f'Downloaded {remote_path} -> {local_path}')
PY
```
