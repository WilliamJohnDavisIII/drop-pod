/* SPDX-License-Identifier: MIT
 * chromehoist: lets Microsoft Edge WebView2 menus show under Wine on macOS. Written for Drop Pod, for Reach
 * Renaissance (Halo: Reach), whose menus are a WebView2 page.
 *
 * Copyright (c) 2026 Devon Fuller
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
 * documentation files (the "Software"), to deal in the Software without restriction, including without limitation
 * the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and
 * to permit persons to whom the Software is furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all copies or substantial portions
 * of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
 * TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL
 * THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF
 * CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
 * DEALINGS IN THE SOFTWARE.
 *
 * Problem: in WebView2 "windowed hosting", the Edge browser process creates its content window (class
 * Chrome_WidgetWin_N) and both the host app and the browser reparent it into the host app's window tree
 * (SetParent). Wine's Mac driver (winemac.drv) cannot composite a window drawn by another process into a foreign
 * process's top-level NSWindow (win32u's update_visible_region leaves such a window with no surface; winemac.drv
 * has FIXME("Cross-process child window Metal swapchains are not implemented")), so Chromium's frames never reach
 * the screen. Three more things stack on top: WebView2 drops --in-process-gpu from the host's additional browser
 * arguments; Edge presents through DirectComposition, which Wine only stubs (dcomp.dll); and Wine's d3dcompiler_47
 * fails the menus' WebGL shaders (install.sh copies Microsoft's from MCC's own CEF3 folder for that one).
 *
 * Fix: this DLL is loaded into the browser process as a proxy for msedge_elf.dll (every export is forwarded to
 * msedge_elf_orig.dll, see msedge_elf.def). In the browser process only, a thread watches for a Chrome_WidgetWin_*
 * window of this process whose parent belongs to another process, lifts it back out as a top-level WS_POPUP window
 * of this process, and keeps it covering the host's child window (same screen rectangle, same visibility). With
 * Chromium's --in-process-gpu switch the swap chain is then created in the same process as the window, which the Mac
 * driver handles like any game window.
 *
 * Stacking: the popup is not WS_EX_TOPMOST (that is a Mac window level above the Dock, over every app) and cannot be
 * owned by the game's window (winemac.drv only links owner and owned NSWindows within one process). It is a plain
 * window that stays behind other apps with the rest of Reach, and is brought back to the front of the game's windows
 * (in the follow loop) while a Reach window has the focus. The title bar and borders of the game's window are not
 * covered: the popup follows the host's child window, which is the game window's client area.
 *
 * Quitting: Cmd-Q or the Dock's Quit sent to the Mac app that owns the popup (the browser process) would end only this
 * process and leave the game and its menu program behind. The popup's window procedure forwards WM_CLOSE,
 * SC_CLOSE and WM_QUERYENDSESSION (what Wine's Quit menu item sends) to the game's window, and plain Cmd-Q (which
 * Wine hands to the program as Alt+Q, its menu item wants Cmd-Option-Q) is caught the same way, which ends every Reach process, and refuses
 * the end of this one. The menu program ending first closes the game window the same way.
 *
 * It also appends --in-process-gpu --disable-direct-composition to the browser's command line (see add_switches)
 * and replaces user32!SetParent in the browser process so Chromium's own reparenting of its content window into
 * the host becomes a no-op (the host's SetParent, from its own process, is undone by the watcher).
 *
 * Logs to %LOCALAPPDATA%\ReachRenaissance\chromehoist.log. CHROMEHOIST=0 turns it off; CHROMEHOIST_CHROME_LOG=1
 * adds Chromium's own log (C:\chromehoist-chrome.log, opens a console window); CHROMEHOIST_SWITCHES="..." adds
 * switches (experiments); CHROMEHOIST_WINDOWS=1 logs every top-level window of the prefix (owner, class, style, size,
 * stacking) when the menus appear or hide and every 5 s. Build: build.sh (Zig's clang, zig cc -target
 * x86_64-windows-gnu). install.sh puts the
 * build into the newest EdgeWebView/Application/<version> folder of Reach Renaissance's prefix, with Edge's own
 * msedge_elf.dll renamed msedge_elf_orig.dll; mods.py prepare puts it back before each start, for when the
 * runtime is updated.
 */
#include <windows.h>
#include <winternl.h>
#include <stdio.h>
#include <stdarg.h>
#include <wchar.h>

typedef struct { HWND w, host, game; BOOL hidden_by_us; } tracked_t;
static tracked_t g_tracked[8];
static int g_ntracked;
static DWORD g_pid;
static HANDLE g_log = INVALID_HANDLE_VALUE;
static HHOOK g_hook;
static BOOL g_winlog;  /* CHROMEHOIST_WINDOWS=1: dump every top-level window now and then (diagnosis) */

static void logline(const char *fmt, ...)
{
    char buf[600]; int n;
    va_list ap; va_start(ap, fmt);
    n = snprintf(buf, sizeof(buf) - 2, "[%lu ms] ", (unsigned long)GetTickCount());
    n += vsnprintf(buf + n, sizeof(buf) - 2 - n, fmt, ap);
    va_end(ap);
    if (n < 0) return;
    /* vsnprintf returns the length it wanted to write. Browser command lines can be much longer than buf. */
    if (n >= (int)sizeof(buf) - 2) n = sizeof(buf) - 3;
    buf[n++] = '\r'; buf[n++] = '\n';
    if (g_log != INVALID_HANDLE_VALUE) { DWORD wr; WriteFile(g_log, buf, n, &wr, NULL); }
}

static void open_log(void)
{
    WCHAR path[MAX_PATH];
    DWORD n = GetEnvironmentVariableW(L"LOCALAPPDATA", path, MAX_PATH);
    if (!n || n >= MAX_PATH - 40) return;
    wcscat(path, L"\\ReachRenaissance");
    CreateDirectoryW(path, NULL);
    wcscat(path, L"\\chromehoist.log");
    g_log = CreateFileW(path, FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
}

static void take(HWND w, HWND host, const char *why);
static BOOL ours(HWND w) { DWORD pid = 0; GetWindowThreadProcessId(w, &pid); return pid == g_pid; }

static BOOL is_widget(HWND w)
{
    WCHAR cls[64];
    if (!GetClassNameW(w, cls, 64)) return FALSE;
    return wcsncmp(cls, L"Chrome_WidgetWin_", 17) == 0;
}

static tracked_t *find_tracked(HWND w)
{
    int i;
    for (i = 0; i < g_ntracked; i++) if (g_tracked[i].w == w) return &g_tracked[i];
    return NULL;
}

/* Cmd-Q or the Dock's Quit, with the menus in front, reaches the browser process (the Mac app that owns the popup),
 * not the game: Wine sends every window of the process WM_QUERYENDSESSION and, if all agree, ends the process, leaving
 * the game and its menu program stuck on a failed-WebView2 dialog. Forward the quit to the game's window instead (that
 * ends everything) and refuse it here, so this process goes down with the rest, in order. */
static LRESULT CALLBACK popup_proc(HWND w, UINT msg, WPARAM wp, LPARAM lp)
{
    WNDPROC old = (WNDPROC)GetPropW(w, L"chromehoist_old");
    HWND game = (HWND)GetPropW(w, L"chromehoist_game");
    if (msg == WM_ENDSESSION) { logline("WM_ENDSESSION at %p (ending %d)", w, (int)wp); if (!wp) return 0; }
    if (msg == WM_CLOSE || msg == WM_QUERYENDSESSION || (msg == WM_SYSCOMMAND && (wp & 0xFFF0) == SC_CLOSE))
    {
        logline("quit request (message %04x) at %p: closing the game window %p", msg, w, game);
        if (game && IsWindow(game))
        {
            PostMessageW(game, WM_CLOSE, 0, 0);
            return msg == WM_QUERYENDSESSION ? FALSE : 0;
        }
    }
    return old ? CallWindowProcW(old, w, msg, wp, lp) : DefWindowProcW(w, msg, wp, lp);
}

/* Plain Cmd-Q: winemac.drv's Quit menu item answers only to Cmd-Option-Q and passes Cmd-Q on to the program as Alt+Q
 * (it maps the Command key to Alt). Catch that in the thread that owns the popup, wherever in its tree the key goes. */
static LRESULT CALLBACK getmsg_hook(int code, WPARAM wp, LPARAM lp)
{
    MSG *m = (MSG *)lp;
    if (code == HC_ACTION && wp == PM_REMOVE && (m->message == WM_SYSKEYDOWN || m->message == WM_KEYDOWN) && m->wParam == 'Q' &&
        ((m->lParam & (1 << 29)) || (GetKeyState(VK_MENU) & 0x8000)) && !(GetKeyState(VK_CONTROL) & 0x8000))
    {
        HWND game = (HWND)GetPropW(GetAncestor(m->hwnd, GA_ROOT), L"chromehoist_game");
        if (game && IsWindow(game))
        {
            logline("Cmd-Q (as Alt+Q) at %p: closing the game window %p", m->hwnd, game);
            PostMessageW(game, WM_CLOSE, 0, 0);
            m->message = WM_NULL;
        }
    }
    return CallNextHookEx(NULL, code, wp, lp);
}

static void subclass_popup(HWND w)
{
    LONG_PTR cur = GetWindowLongPtrW(w, GWLP_WNDPROC);
    if (GetPropW(w, L"chromehoist_old") || cur == (LONG_PTR)popup_proc) return;
    SetPropW(w, L"chromehoist_old", (HANDLE)cur);
    SetWindowLongPtrW(w, GWLP_WNDPROC, (LONG_PTR)popup_proc);
    logline("subclassed %p (was %p, now %p)", w, (void *)cur, (void *)GetWindowLongPtrW(w, GWLP_WNDPROC));
    if (!g_hook)
    {
        g_hook = SetWindowsHookExW(WH_GETMESSAGE, getmsg_hook, NULL, GetWindowThreadProcessId(w, NULL));
        logline("Cmd-Q hook %s", g_hook ? "installed" : "not installed");
    }
}

/* Lift w (a child of host, a window of another process) out as a top-level popup covering host. */
static void hoist(HWND w, HWND host)
{
    RECT r;
    LONG_PTR style = GetWindowLongPtrW(w, GWL_STYLE), ex = GetWindowLongPtrW(w, GWL_EXSTYLE);
    LONG_PTR nstyle = (style & ~(WS_CHILD | WS_CAPTION | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU | WS_BORDER | WS_DLGFRAME))
                      | WS_POPUP | WS_CLIPCHILDREN | WS_CLIPSIBLINGS;
    SetWindowLongPtrW(w, GWL_STYLE, nstyle);
    SetWindowLongPtrW(w, GWL_EXSTYLE, ex & ~(WS_EX_TOPMOST | WS_EX_APPWINDOW));
    SetParent(w, NULL);
    GetWindowRect(host, &r);
    SetWindowPos(w, HWND_TOP, r.left, r.top, r.right - r.left, r.bottom - r.top,
                 SWP_NOACTIVATE | SWP_FRAMECHANGED | (style & WS_VISIBLE ? SWP_SHOWWINDOW : 0));
    subclass_popup(w);
    logline("hoisted %p (style %08lx -> %08lx) out of host child %p; placed at %ld,%ld %ldx%ld",
            w, (long)style, (long)nstyle, host, r.left, r.top, r.right - r.left, r.bottom - r.top);
}

static BOOL CALLBACK scan_cb(HWND w, LPARAM lp)
{
    HWND parent;
    if (!ours(w) || !is_widget(w)) return TRUE;
    parent = GetAncestor(w, GA_PARENT);
    if (!parent || parent == GetDesktopWindow() || ours(parent)) return TRUE;
    take(w, parent, "scan");
    return TRUE;
}

static void exe_name(DWORD pid, WCHAR *out, DWORD cap)
{
    HANDLE h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
    DWORD n = MAX_PATH;
    WCHAR full[MAX_PATH];
    out[0] = 0;
    if (!h) return;
    if (QueryFullProcessImageNameW(h, 0, full, &n)) { WCHAR *s = wcsrchr(full, L'\\'); wcsncpy(out, s ? s + 1 : full, cap - 1); out[cap - 1] = 0; }
    CloseHandle(h);
}

static void dump_one(HWND w, const char *tag)
{
    WCHAR cls[64] = L"", title[64] = L"", exe[64];
    RECT r = { 0 };
    DWORD pid = 0;
    GetClassNameW(w, cls, 64);
    GetWindowTextW(w, title, 64);
    GetWindowRect(w, &r);
    GetWindowThreadProcessId(w, &pid);
    exe_name(pid, exe, 64);
    logline("  %s %p %ls pid %lu class '%ls' title '%ls' style %08lx ex %08lx vis %d %ld,%ld %ldx%ld parent %p owner %p", tag, w, exe, (unsigned long)pid,
            cls, title, (unsigned long)GetWindowLongPtrW(w, GWL_STYLE), (unsigned long)GetWindowLongPtrW(w, GWL_EXSTYLE), IsWindowVisible(w),
            r.left, r.top, r.right - r.left, r.bottom - r.top, GetAncestor(w, GA_PARENT), GetWindow(w, GW_OWNER));
}

static BOOL CALLBACK dump_cb(HWND w, LPARAM lp) { dump_one(w, "top"); return TRUE; }

/* CHROMEHOIST_WINDOWS=1: every top-level window of the desktop in z-order (front first), plus the menu window, the host
 * child and its top-level of each tracked window. */
static void dump_windows(const char *why)
{
    int i;
    if (!g_winlog) return;
    logline("WINDOWS (%s), foreground %p", why, GetForegroundWindow());
    for (i = 0; i < g_ntracked; i++)
    {
        HWND root = GetAncestor(g_tracked[i].host, GA_ROOT);
        dump_one(g_tracked[i].w, "menu");
        dump_one(g_tracked[i].host, "hostchild");
        if (root && root != g_tracked[i].host) dump_one(root, "hostroot");
    }
    EnumWindows(dump_cb, 0);
}

/* The game's own top-level window: the owner of the host's top-level (menu-overlay's popup), else that popup itself. */
static HWND game_of(HWND host)
{
    HWND root = GetAncestor(host, GA_ROOT), owner = root ? GetWindow(root, GW_OWNER) : NULL;
    return owner ? owner : root;
}

/* Is a window of the game, its menu program or this browser the foreground window? (It is 0 while another Mac app is.) */
static BOOL reach_active(tracked_t *t)
{
    DWORD fgpid = 0, gpid = 0;
    HWND fg = GetForegroundWindow();
    if (!fg) return FALSE;
    GetWindowThreadProcessId(fg, &fgpid);
    if (t->game) GetWindowThreadProcessId(t->game, &gpid);
    return fgpid == g_pid || (gpid && fgpid == gpid) || GetAncestor(fg, GA_ROOT) == GetAncestor(t->host, GA_ROOT);
}

static BOOL below_game(tracked_t *t)
{
    HWND p, root = GetAncestor(t->host, GA_ROOT);
    for (p = GetWindow(t->w, GW_HWNDPREV); p; p = GetWindow(p, GW_HWNDPREV))
        if (p == t->game || p == root) return TRUE;
    return FALSE;
}

static void follow(tracked_t *t)
{
    RECT hr, wr;
    HWND p;
    if (!IsWindow(t->w) || !IsWindow(t->host))
    {
        logline("window %p or host %p gone", t->w, t->host);
        /* the menu program ending first (its own Cmd-Q, a crash) must not leave the game running without menus */
        if (t->game && IsWindow(t->game)) { PostMessageW(t->game, WM_CLOSE, 0, 0); logline("closing the game window %p", t->game); }
        *t = g_tracked[--g_ntracked];
        return;
    }
    p = GetAncestor(t->w, GA_PARENT);
    if (p && p != GetDesktopWindow() && !ours(p)) { t->host = p; hoist(t->w, p); }
    t->game = game_of(t->host);
    SetPropW(t->w, L"chromehoist_game", t->game);
    if (!IsWindowVisible(t->host))
    {
        if (IsWindowVisible(t->w)) { ShowWindow(t->w, SW_HIDE); t->hidden_by_us = TRUE; logline("host hidden: hid %p", t->w); dump_windows("host hidden"); }
        return;
    }
    if (t->hidden_by_us) { ShowWindow(t->w, SW_SHOWNA); t->hidden_by_us = FALSE; logline("host visible again: showed %p", t->w); dump_windows("host visible"); }
    GetWindowRect(t->host, &hr);
    GetWindowRect(t->w, &wr);
    if (!EqualRect(&hr, &wr))
        SetWindowPos(t->w, 0, hr.left, hr.top, hr.right - hr.left, hr.bottom - hr.top, SWP_NOACTIVATE | SWP_NOZORDER);
    /* Stacking: the popup belongs to another process than the game window, and winemac.drv can neither own nor order
     * windows across processes, so it is simply brought to the front of its level whenever it falls behind the
     * game's windows while the game is in front. Not while another app is: then it stays where the Mac put it. */
    if (reach_active(t) && below_game(t))
    {
        SetWindowPos(t->w, HWND_TOP, 0, 0, 0, 0, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE);
        logline("brought %p back above the game window %p", t->w, t->game);
    }
}

/* Take over w (Chromium's content window of this process) as a top-level popup that covers host. */
static void take(HWND w, HWND host, const char *why)
{
    tracked_t *t = find_tracked(w);
    if (!t)
    {
        if (g_ntracked >= 8) return;
        t = &g_tracked[g_ntracked++];
        t->w = w; t->hidden_by_us = FALSE; t->game = NULL;
        logline("%s: taking %p (host child %p)", why, w, host);
    }
    t->host = host;
    hoist(w, host);
    dump_windows(why);
}

typedef HWND (WINAPI *NtUserSetParent_t)(HWND, HWND);
static NtUserSetParent_t real_set_parent;

static LONG g_sp_calls;
static HWND WINAPI hooked_SetParent(HWND hwnd, HWND parent)
{
    if (InterlockedIncrement(&g_sp_calls) <= 20) logline("SetParent(%p, %p) called (hwnd ours %d, parent ours %d)", hwnd, parent, ours(hwnd), parent ? ours(parent) : -1);
    if (parent && parent != GetDesktopWindow() && ours(hwnd) && !ours(parent) && is_widget(hwnd))
    {
        HWND before = GetAncestor(hwnd, GA_PARENT);
        logline("SetParent(%p, foreign %p) refused", hwnd, parent);
        take(hwnd, parent, "SetParent");
        return before ? before : GetDesktopWindow();
    }
    return real_set_parent(hwnd, parent);
}

/* user32!SetParent in Wine is a plain call to win32u!NtUserSetParent: replace its first bytes with a jump to
 * hooked_SetParent and call NtUserSetParent ourselves for the calls we let through. */
static void install_set_parent_hook(void)
{
    HMODULE user32 = LoadLibraryW(L"user32.dll"), win32u = GetModuleHandleW(L"win32u.dll");
    BYTE *target = user32 ? (BYTE *)GetProcAddress(user32, "SetParent") : NULL;
    DWORD old;
    if (!win32u) win32u = LoadLibraryW(L"win32u.dll");
    real_set_parent = win32u ? (NtUserSetParent_t)GetProcAddress(win32u, "NtUserSetParent") : NULL;
    if (!target || !real_set_parent) { logline("SetParent hook not installed (user32 %p win32u %p)", user32, win32u); return; }
    logline("user32!SetParent at %p (bytes %02x %02x %02x %02x %02x %02x %02x %02x), win32u!NtUserSetParent at %p", target,
            target[0], target[1], target[2], target[3], target[4], target[5], target[6], target[7], (void *)real_set_parent);
    if ((BYTE *)real_set_parent == target) { logline("SetParent is a forwarder to NtUserSetParent: not hooking"); return; }
    if (VirtualProtect(target, 16, PAGE_EXECUTE_READWRITE, &old))
    {
        BYTE jmp[12] = { 0x48, 0xB8, 0,0,0,0,0,0,0,0, 0xFF, 0xE0 };  /* mov rax, imm64; jmp rax */
        *(void **)(jmp + 2) = (void *)hooked_SetParent;
        memcpy(target, jmp, sizeof(jmp));
        VirtualProtect(target, 16, old, &old);
        FlushInstructionCache(GetCurrentProcess(), target, 16);
        logline("SetParent hook installed at %p", target);
    }
    else logline("SetParent hook: VirtualProtect failed (%lu)", GetLastError());
}

/* WebView2 does not pass the host's "additional browser arguments" on the command line (it applies them later,
 * inside the browser, and drops --in-process-gpu). Chromium reads its switches with GetCommandLineW(), which in
 * Wine's kernelbase returns a pointer cached at startup (mov rax,[rip+slot]; ret): point that slot (and the PEB) at a
 * copy with our switches appended, before msedge.dll starts.
 *   --in-process-gpu            the swap chain is created in this process, the one that owns the hoisted window
 *   --disable-direct-composition  Wine's dcomp.dll is a stub: frames presented through it are never shown
 *   --enable-logging --v=1 --log-file  Chromium's own log (C:\chromehoist-chrome.log), only with CHROMEHOIST_CHROME_LOG=1:
 *                                 it also opens a Wine console window
 *   CHROMEHOIST_SWITCHES="..."     any further switches (experiments) */
static void add_switches(void)
{
    UNICODE_STRING *cl = &NtCurrentTeb()->ProcessEnvironmentBlock->ProcessParameters->CommandLine;
    static const WCHAR add[] = L" --in-process-gpu --disable-direct-composition";
    static const WCHAR addlog[] = L" --enable-logging --v=1 --log-file=C:\\chromehoist-chrome.log";
    WCHAR extra[512] = L"";  /* CHROMEHOIST_SWITCHES: more Chromium switches, for experiments */
    DWORD extralen = GetEnvironmentVariableW(L"CHROMEHOIST_SWITCHES", extra + 1, 510);
    if (extralen && extralen < 510) extra[0] = L' '; else extra[0] = 0;
    HMODULE kb = GetModuleHandleW(L"kernelbase.dll");
    BYTE *fn = kb ? (BYTE *)GetProcAddress(kb, "GetCommandLineW") : NULL;
    WCHAR **slot = NULL, *old, *buf, nolog[4];
    size_t oldlen, n;
    BOOL want_log = GetEnvironmentVariableW(L"CHROMEHOIST_CHROME_LOG", nolog, 4) && nolog[0] == L'1';
    if (fn && fn[0] == 0x48 && fn[1] == 0x8B && fn[2] == 0x05) slot = (WCHAR **)(fn + 7 + *(INT32 *)(fn + 3));
    old = slot ? *slot : cl->Buffer;
    if (!old || wcsstr(old, L"--in-process-gpu")) return;
    oldlen = wcslen(old);
    n = oldlen + wcslen(add) + (want_log ? wcslen(addlog) : 0) + wcslen(extra) + 1;
    buf = HeapAlloc(GetProcessHeap(), 0, n * sizeof(WCHAR));
    if (!buf) return;
    wcscpy(buf, old); wcscat(buf, add); if (want_log) wcscat(buf, addlog); wcscat(buf, extra);
    if (slot) *slot = buf;
    cl->Buffer = buf;
    cl->Length = (USHORT)((n - 1) * sizeof(WCHAR));
    cl->MaximumLength = cl->Length + sizeof(WCHAR);
    logline("command line %s: %ls", slot ? "(kernelbase slot + PEB)" : "(PEB only)", buf + oldlen);
}

static DWORD WINAPI watchdog(void *arg)
{
    int i;
    DWORD last_dump = 0;
    install_set_parent_hook();
    logline("watching for WebView windows parented into other processes (pid %lu)", (unsigned long)g_pid);
    for (;;)
    {
        EnumChildWindows(GetDesktopWindow(), scan_cb, 0);
        for (i = 0; i < g_ntracked; i++) follow(&g_tracked[i]);
        if (g_winlog && g_ntracked && GetTickCount() - last_dump >= 5000) { last_dump = GetTickCount(); dump_windows("periodic"); }
        Sleep(15);
    }
    return 0;
}

BOOL WINAPI DllMain(HINSTANCE inst, DWORD reason, void *reserved)
{
    if (reason == DLL_PROCESS_ATTACH)
    {
        const WCHAR *cl = GetCommandLineW();
        WCHAR off[8];
        DisableThreadLibraryCalls(inst);
        if (GetEnvironmentVariableW(L"CHROMEHOIST", off, 8) && off[0] == L'0') return TRUE;
        /* the browser process only: the one started for an embedded WebView, with no --type= (renderer, GPU, ...) */
        if (!cl || !wcsstr(cl, L"--embedded-browser-webview") || wcsstr(cl, L"--type=")) return TRUE;
        g_pid = GetCurrentProcessId();
        g_winlog = GetEnvironmentVariableW(L"CHROMEHOIST_WINDOWS", off, 8) && off[0] == L'1';
        open_log();
        logline("loaded in browser process: %ls", cl);
        add_switches();
        CloseHandle(CreateThread(NULL, 0, watchdog, NULL, 0, NULL));
    }
    return TRUE;
}
