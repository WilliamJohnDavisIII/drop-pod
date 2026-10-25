#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Devon Fuller
"""exports.py <msedge_elf.dll> [proxy name] [forward-to name]: write msedge_elf.def, the module definition that makes
the chromehoist proxy forward every export of Edge's msedge_elf.dll to the original, renamed msedge_elf_orig.dll.
Run it against the msedge_elf.dll of the WebView2 runtime install.sh pins (EdgeWebView/Application/<version>/), then
build.sh. Reads the PE export table itself (no dumpbin or objdump needed). Exports by ordinal only are forwarded by
ordinal; msedge.dll imports msedge_elf.dll by name, so a missing export would stop the browser process loading."""
import struct
import sys


def exports(path):
    data = open(path, "rb").read()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    assert data[pe:pe + 4] == b"PE\0\0", "not a PE file"
    sections = struct.unpack_from("<H", data, pe + 6)[0]
    opt = pe + 24
    magic = struct.unpack_from("<H", data, opt)[0]
    dirs = opt + (112 if magic == 0x20B else 96)  # the data directories, after the PE32+ or PE32 optional header
    export_rva, export_size = struct.unpack_from("<II", data, dirs)
    opt_size = struct.unpack_from("<H", data, pe + 20)[0]
    table = []
    for i in range(sections):
        s = opt + opt_size + i * 40
        vsize, va, rawsize, raw = struct.unpack_from("<IIII", data, s + 8)
        table.append((va, max(vsize, rawsize), raw))

    def off(rva):
        for va, size, raw in table:
            if va <= rva < va + size:
                return raw + rva - va
        raise ValueError(f"rva {rva:#x} is in no section")

    def cstr(rva):
        o = off(rva)
        return data[o:data.index(b"\0", o)].decode()

    e = off(export_rva)
    base, n_funcs, n_names, funcs, names, ords = struct.unpack_from("<IIIIII", data, e + 16)
    named = {}
    for i in range(n_names):
        name_rva = struct.unpack_from("<I", data, off(names) + 4 * i)[0]
        named[struct.unpack_from("<H", data, off(ords) + 2 * i)[0]] = cstr(name_rva)
    out = []
    for i in range(n_funcs):
        if struct.unpack_from("<I", data, off(funcs) + 4 * i)[0]:  # (an unused ordinal has no address)
            out.append((base + i, named.get(i)))
    return out


def source(path):
    """<WebView2 version folder>/<file name>, for the def's comment."""
    import pathlib
    p = pathlib.Path(path).resolve()
    return f"{p.parent.name}/{p.name}"


def main(argv):
    if len(argv) not in (2, 3, 4):
        sys.exit(__doc__)
    proxy = argv[2] if len(argv) > 2 else "msedge_elf.dll"
    orig = (argv[3] if len(argv) > 3 else "msedge_elf_orig.dll").removesuffix(".dll")
    lines = ["; chromehoist (MIT, Copyright (c) 2026 Devon Fuller): every export of Edge's msedge_elf.dll, forwarded",
             f"; to the original, renamed {orig}.dll. Made by exports.py from {source(argv[1])}; see build.sh.",
             f"LIBRARY {proxy}", "EXPORTS"]
    for ordinal, name in exports(argv[1]):
        if name:
            lines.append(f"    {name}={orig}.{name} @{ordinal}")
        else:
            lines.append(f"    ord{ordinal}={orig}.#{ordinal} @{ordinal} NONAME")
    print("\n".join(lines))


if __name__ == "__main__":
    main(sys.argv)
