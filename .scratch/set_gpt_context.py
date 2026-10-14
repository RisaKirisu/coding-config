import re, sys

for path in sys.argv[1:]:
    out = []
    in_gpt = False
    for line in open(path).read().split("\n"):
        m = re.match(r"^(\s+)- id: (\S+)", line)
        if m:
            in_gpt = m.group(2).startswith("gpt-")
            out.append(line)
            if in_gpt:
                out.append(f"{m.group(1)}  contextWindow: 500000")
        elif in_gpt and re.match(r"^\s+contextWindow:", line):
            continue
        else:
            out.append(line)
    open(path, "w").write("\n".join(out))
