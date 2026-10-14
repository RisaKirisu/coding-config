from pathlib import Path

def replace(path, old, new):
    p = Path(path)
    text = p.read_text()
    assert text.count(old) == 1, (path, old)
    p.write_text(text.replace(old, new))

replace("devvm", """            create
            start
            run_smolvm smolvm_exec_args stream "$@"
""", """            run_smolvm smolvm_exec_args stream "$@"
""")
replace("src/runtime.rs", """        // `devvm exec` would create and start a DevVM, so never probe a stopped or
        // unobservable VM. A failed VM probe is not evidence that DSH is stopped.
""", """        // A stopped DevVM has no DSH Runtime, so only a running VM is probed. A failed VM
        // probe is not evidence that DSH is stopped.
""")
replace("src/runtime.rs", """        // A stopped DevVM has no DSH Runtime, and `devvm exec` would start the DevVM to look.
""", """        // A stopped DevVM has no DSH Runtime to stop.
""")
replace("src/api.rs", """    // `devvm exec` auto-creates and starts a DevVM, so the runner must never be called
    // for a Project whose DevVM is not already running.
""", """    // Sync Status lives inside the DevVM, so it is read only while the DevVM is running.
""")
replace("readme.md", "| `devvm exec -- pwd` | Run a guest command; the Project is mounted at `/root/workspace` |",
        "| `devvm exec -- pwd` | Run a guest command in the running microVM; the Project is mounted at `/root/workspace` |")
