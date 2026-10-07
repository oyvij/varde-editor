import fcntl, os, pty, select, signal, struct, sys, termios, time

if "--" not in sys.argv or len(sys.argv) < 5:
    sys.exit("usage: drive.py OUT ROWS COLS [STEP...] -- COMMAND [ARG...]\n"
             "  STEP is @MS to wait, or keys with Python escapes: j  '\\r'  '\\x1b'  ':q\\r'")
split = sys.argv.index("--")
out, rows, cols, steps, command = (sys.argv[1], int(sys.argv[2]), int(sys.argv[3]),
                                   sys.argv[4:split], sys.argv[split + 1:])

pid, fd = pty.fork()
if pid == 0:
    os.execvp(command[0], command)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))

captured = bytearray()
replies = {b"\x1b]10;?\x07": b"\x1b]10;rgb:ffff/ffff/ffff\x07",
           b"\x1b]11;?\x07": b"\x1b]11;rgb:0000/0000/0000\x07",
           b"\x1b[c": b"\x1b[?62;22c",
           b"\x1b[6n": b"\x1b[1;1R"}

def drain(seconds):
    end = time.monotonic() + seconds
    while (left := end - time.monotonic()) > 0:
        if select.select([fd], [], [], left)[0]:
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                return
            captured.extend(chunk)
            for query, reply in replies.items():
                for _ in range(chunk.count(query)):
                    os.write(fd, reply)

for step in steps + ["@300"]:
    if step.startswith("@"):
        drain(int(step[1:]) / 1000)
    else:
        os.write(fd, step.encode().decode("unicode_escape").encode("latin-1"))
        drain(0.05)

os.kill(pid, signal.SIGKILL)
os.waitpid(pid, 0)
open(out, "wb").write(captured)
