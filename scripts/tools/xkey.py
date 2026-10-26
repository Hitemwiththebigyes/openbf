#!/usr/bin/env python3
"""xkey.py <display-number> key <KeysymHex> | close | list

Raw X11 (no python-xlib / xdotool on this machine): focus the largest mapped
top-level window of an Xvfb started with -ac, then send it a key press through
XTEST, or a WM_DELETE_WINDOW client message. Card BLKB's acceptance run.
F9 = 0xffc6.
"""
import socket
import struct
import sys


class X:
    def __init__(self, n):
        self.s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.s.connect(f"/tmp/.X11-unix/X{n}")
        self.s.sendall(struct.pack("<BxHHHHxx", ord("l"), 11, 0, 0, 0))
        head = self.read(8)
        if head[0] != 1:
            raise SystemExit(f"X setup refused: {self.read(struct.unpack('<H', head[6:8])[0] * 4)!r}")
        extra = self.read(struct.unpack("<H", head[6:8])[0] * 4)
        vendor_len = struct.unpack("<H", extra[16:18])[0]
        nformats = extra[21]
        self.min_kc, self.max_kc = extra[26], extra[27]
        at = 32 + (vendor_len + 3) // 4 * 4 + 8 * nformats
        self.root = struct.unpack("<I", extra[at:at + 4])[0]

    def read(self, n):
        b = b""
        while len(b) < n:
            c = self.s.recv(n - len(b))
            if not c:
                raise SystemExit("X closed the connection")
            b += c
        return b

    def reply(self):
        while True:
            h = self.read(32)
            if h[0] == 1:
                return h + self.read(struct.unpack("<I", h[4:8])[0] * 4)
            if h[0] == 0:
                raise SystemExit(f"X error code {h[1]} for opcode {h[10]}")

    def req(self, op, data_byte, body):
        pad = (-len(body)) % 4
        self.s.sendall(struct.pack("<BBH", op, data_byte, 1 + (len(body) + pad) // 4) + body + b"\0" * pad)

    def sync(self):
        self.req(43, 0, b"")  # GetInputFocus
        return self.reply()

    def children(self, w):
        self.req(15, 0, struct.pack("<I", w))
        r = self.reply()
        n = struct.unpack("<H", r[16:18])[0]
        return list(struct.unpack(f"<{n}I", r[32:32 + 4 * n]))

    def geometry(self, w):
        self.req(14, 0, struct.pack("<I", w))
        r = self.reply()
        return struct.unpack("<HH", r[16:20])

    def mapped(self, w):
        self.req(3, 0, struct.pack("<I", w))  # GetWindowAttributes
        return self.reply()[26] == 2

    def window(self):
        best, area = None, 0
        for w in self.children(self.root):
            if not self.mapped(w):
                continue
            gw, gh = self.geometry(w)
            if gw * gh > area:
                best, area = w, gw * gh
        return best

    def atom(self, name):
        b = name.encode()
        self.req(16, 0, struct.pack("<HH", len(b), 0) + b)
        return struct.unpack("<I", self.reply()[8:12])[0]

    def keycode(self, keysym):
        count = self.max_kc - self.min_kc + 1
        self.req(101, 0, struct.pack("<BBH", self.min_kc, count, 0))
        r = self.reply()
        per = r[1]
        syms = struct.unpack(f"<{per * count}I", r[32:32 + 4 * per * count])
        for k in range(count):
            if keysym in syms[k * per:(k + 1) * per]:
                return self.min_kc + k
        raise SystemExit(f"no keycode for keysym {keysym:#x}")

    def xtest(self):
        name = b"XTEST"
        self.req(98, 0, struct.pack("<HH", len(name), 0) + name)
        r = self.reply()
        if not r[8]:
            raise SystemExit("no XTEST extension")
        return r[9]

    def focus(self, w):
        self.req(42, 2, struct.pack("<II", w, 0))  # SetInputFocus, RevertToParent
        self.sync()

    def key(self, keysym):
        major, kc = self.xtest(), self.keycode(keysym)
        for kind in (2, 3):  # KeyPress, KeyRelease
            body = struct.pack("<BBHII8xhh7xB", kind, kc, 0, 0, 0, 0, 0, 0)
            self.s.sendall(struct.pack("<BBH", major, 2, 9) + body)
            self.sync()

    def close(self, w):
        protocols, delete = self.atom("WM_PROTOCOLS"), self.atom("WM_DELETE_WINDOW")
        event = struct.pack("<BBHII5I", 33, 32, 0, w, protocols, delete, 0, 0, 0, 0)
        self.req(25, 0, struct.pack("<II", w, 0) + event)  # SendEvent
        self.sync()


def main():
    x = X(int(sys.argv[1]))
    w = x.window()
    if sys.argv[2] == "list":
        print("root", hex(x.root), "window", w and hex(w), w and x.geometry(w))
        return
    if w is None:
        raise SystemExit("no mapped window")
    if sys.argv[2] == "key":
        x.focus(w)
        x.key(int(sys.argv[3], 16))
    elif sys.argv[2] == "close":
        x.close(w)
    print("ok", sys.argv[2], hex(w))


main()
