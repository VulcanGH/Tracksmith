/* Test-only MediaInfo SDK stub: deterministic delays, facts and call counts. */
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <wchar.h>

__declspec(dllexport) void *__stdcall MediaInfo_New(void) {
    return malloc(1);
}

__declspec(dllexport) void __stdcall MediaInfo_Delete(void *handle) {
    free(handle);
}

__declspec(dllexport) const wchar_t *__stdcall MediaInfo_Option(
    void *handle, const wchar_t *option, const wchar_t *value
) {
    (void)handle; (void)option; (void)value;
    return L"";
}

__declspec(dllexport) size_t __stdcall MediaInfo_Open(
    void *handle, const wchar_t *path
) {
    (void)handle;
    const wchar_t *log = _wgetenv(L"TRACKSMITH_TEST_MI_LOG");
    if (log) {
        FILE *file = _wfopen(log, L"a");
        if (file) { fputs("open\n", file); fclose(file); }
    }
    const wchar_t *delay = _wgetenv(L"TRACKSMITH_TEST_MI_DELAY");
    DWORD milliseconds = delay ? (DWORD)_wtoi(delay) : 500;
    if (wcsstr(path, L"audio.mka")) milliseconds = 0;
    Sleep(milliseconds);
    return 1;
}

__declspec(dllexport) const wchar_t *__stdcall MediaInfo_Inform(
    void *handle, size_t reserved
) {
    (void)handle; (void)reserved;
    const wchar_t *facts = _wgetenv(L"TRACKSMITH_TEST_MI_JSON");
    return facts ? facts : L"{}";
}
