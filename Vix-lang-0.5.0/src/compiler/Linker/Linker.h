#ifndef VIX_LINKER_H
#define VIX_LINKER_H

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    const char* target_triple;
    int bare_mode;
    const char* linker_script;
    int static_link;
    const char* entry_point;
    const char* libc_dir;     /* directory containing bundled libc (CRT objects, DLLs, static libs) */
    const char** lib_paths;   /* additional library search paths from -L */
    int lib_path_count;       /* number of library search paths */
    const char** extra_libs;  /* additional libraries from -l */
    int extra_lib_count;      /* number of additional libraries */
} VixLinkOptions;

int vix_link(const char* obj_file, const char* output_file,
             const VixLinkOptions* options, const char** error_msg);

int vix_link_multi(const char** obj_files, int obj_count,
                   const char* output_file,
                   const VixLinkOptions* options, const char** error_msg);

#ifdef __cplusplus
}
#endif

#endif
