#!/usr/bin/env python3
"""Types into the test VM through Lume's VNC server (host side).

    python3 -I tests/e2e/vnc_keys.py <vnc url> <text> [<text>...]

Each <text> is typed as key presses; the word RETURN presses Return. The URL
is what `lume get <vm> -f json` reports as vncUrl
(vnc://:<password>@127.0.0.1:<port>). Prints "typed <n> keys".

Why: macOS's password sheets (granting Accessibility in System Settings)
refuse key events posted by software (CGEventPost, cua-driver), so a
scenario couldn't grant Accessibility at all (README, "macOS notes"). Keys
sent over VNC arrive through the VM's virtual keyboard, as a person's would.
Used by e2e_accessibility_revoked (M5.9 criterion 6, #173) to type the
throwaway VM user's password into that sheet; never point it at anything but
a test VM's VNC server.

A minimal RFB 3.8 client: the "None" or "VNC Authentication" security type,
then KeyEvent messages only. VNC Authentication is DES of the server's
challenge with the password (bit-reversed bytes) as the key; DES is
TripleDES with one 8-byte key, from the `cryptography` package.
"""

import socket
import struct
import sys
import time
from urllib.parse import urlsplit

RETURN = 0xFF0D


def recv_exact(sock, n):
    data = b""
    while len(data) < n:
        chunk = sock.recv(n - len(data))
        if not chunk:
            raise RuntimeError("the VNC server closed the connection")
        data += chunk
    return data


def des_response(password, challenge):
    # Imported here so the usage error needs no third-party package.
    from cryptography.hazmat.decrepit.ciphers.algorithms import TripleDES
    from cryptography.hazmat.primitives.ciphers import Cipher, modes

    key = bytes(int(f"{b:08b}"[::-1], 2) for b in password.encode()[:8].ljust(8, b"\0"))
    encryptor = Cipher(TripleDES(key), modes.ECB()).encryptor()
    return encryptor.update(challenge) + encryptor.finalize()


def connect(url):
    parts = urlsplit(url)
    if parts.scheme != "vnc" or not parts.hostname or not parts.port:
        raise RuntimeError("not a vnc://:<password>@<host>:<port> URL")
    sock = socket.create_connection((parts.hostname, parts.port), timeout=10)
    version = recv_exact(sock, 12)
    if not version.startswith(b"RFB 003."):
        raise RuntimeError(f"not an RFB server: {version!r}")
    sock.sendall(b"RFB 003.008\n")
    count = recv_exact(sock, 1)[0]
    if count == 0:
        length = struct.unpack(">I", recv_exact(sock, 4))[0]
        raise RuntimeError(recv_exact(sock, length).decode(errors="replace"))
    types = recv_exact(sock, count)
    if 2 in types:
        sock.sendall(b"\x02")
        challenge = recv_exact(sock, 16)
        sock.sendall(des_response(parts.password or "", challenge))
    elif 1 in types:
        sock.sendall(b"\x01")
    else:
        raise RuntimeError(f"no supported security type in {list(types)}")
    if struct.unpack(">I", recv_exact(sock, 4))[0] != 0:
        raise RuntimeError("VNC authentication failed")
    sock.sendall(b"\x01")  # ClientInit: shared
    recv_exact(sock, 20)  # width, height, pixel format
    recv_exact(sock, struct.unpack(">I", recv_exact(sock, 4))[0])  # name
    return sock


def key(sock, keysym):
    for down in (1, 0):
        sock.sendall(struct.pack(">BBHI", 4, down, 0, keysym))
        time.sleep(0.04)


def main(argv):
    if len(argv) < 3:
        print(__doc__.strip().splitlines()[2].strip(), file=sys.stderr)
        return 2
    sock = connect(argv[1])
    typed = 0
    try:
        for text in argv[2:]:
            keysyms = [RETURN] if text == "RETURN" else [ord(c) for c in text]
            for keysym in keysyms:
                if keysym != RETURN and not 0x20 <= keysym < 0x7F:
                    raise RuntimeError("only printable ASCII and RETURN")
                key(sock, keysym)
                typed += 1
    finally:
        sock.close()
    print(f"typed {typed} keys")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
