/*
 * Copyright (c) 2026 Vix Language Authors. All rights reserved.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "../include/ast.h"
#include "../include/codegen.h"
#include "../include/compat.h"
#include "../include/compiler.h"
#include "../include/macro.h"
#include "../include/ownership.h"
#include "../include/parser.h"
#include "../include/semantic.h"
#include "../include/typeck.h"
#include "compiler/Linker/Linker.h"
#include "compiler/Llc/Llc.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#include <windows.h>
#include <time.h>

static int vix_clock_gettime(int unused, struct timespec *ts) {
    (void)unused;
    LARGE_INTEGER freq, counter;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&counter);
    ts->tv_sec = (long)(counter.QuadPart / freq.QuadPart);
    ts->tv_nsec = (long)((counter.QuadPart % freq.QuadPart) * 1000000000 / freq.QuadPart);
    return 0;
}
#define CLOCK_MONOTONIC 1
#define clock_gettime vix_clock_gettime
#else
#include <libgen.h>
#include <limits.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
#endif

extern FILE *yyin;
extern ASTNode *root;
const char *current_input_filename = NULL;

#define MAX_OBJ_FILES 256
#define MAX_LIB_PATHS 64
#define MAX_EXTRA_LIBS 64

/** All parsed CLI options, stored as a struct for clean passing between phases */
typedef struct {
  char *in_f;
  char *out_f;
  char *llvm_f;
  char *obj_f;
  char *asm_f;
  char *target;
  int is_vic;
  int save_c;
  int gen_llvm;
  int gen_obj;
  int gen_asm;
  int out_ast;
  int out_llvm;
  int dbg;
  int opt_level;
  int no_std;
  int no_main;
  int check_only;
  int show_time;
  int link_mode;
  int static_link;
  char *obj_files[MAX_OBJ_FILES];
  int obj_file_count;
  char *lib_paths[MAX_LIB_PATHS];
  int lib_path_count;
  char *extra_libs[MAX_EXTRA_LIBS];
  int extra_lib_count;
} VixOptions;

/* ── Forward declarations ──────────────────── */
static void print_usage(const char *prog);
static int parse_args(VixOptions *opts, int argc, char **argv);
static int run_linker_mode(const VixOptions *opts);
static int run_compiler_pipeline(const VixOptions *opts, FILE *input_file);
static void print_timing_table(struct timespec t_start, struct timespec t_file,
                               struct timespec t_parse, struct timespec t_sema,
                               struct timespec t_codegen);
static const char *find_bundled_libc(void);

/* extern C wrappers from compiler/Attrs.cpp */
extern int vix_source_get_attrs(const char *filePath, int *out_no_std, int *out_no_main);
/* parser global state reset (parser.y) */
extern void vix_reset_parser_state(void);

/* ==================================================================
 * main — entry point
 * ================================================================== */
int main(int argc, char **argv) {
  VixOptions opts;
  memset(&opts, 0, sizeof(opts));

  if (argc < 2) {
    print_usage(argv[0]);
    return 1;
  }

  if (!parse_args(&opts, argc, argv))
    return 1; /* parse_args already printed the error */

  vix_setenv("VIX_DEBUG", opts.dbg ? "1" : "0", 1);
  vix_set_opt_level(opts.opt_level);

  /* ── Linker mode: link .o files into executable ── */
  if (opts.link_mode && opts.obj_file_count > 0)
    return run_linker_mode(&opts);

  /* ── Compiler mode: source → LLVM IR / object / executable ── */
  if (!opts.in_f) {
    fprintf(stderr, "Error: no input file specified\n");
    return 1;
  }

  FILE *input_file = fopen(opts.in_f, "r");
  if (!input_file) {
    perror("Failed to open file");
    return 1;
  }

  FILE *preprocessed_file = vix_preprocess_macros(input_file, opts.in_f);
  if (!preprocessed_file) {
    fclose(input_file);
    return 1;
  }

  /* Scan for #[no_std] / #[no_main] attributes via source parser (one file read) */
  vix_source_get_attrs(opts.in_f, &opts.no_std, &opts.no_main);

  /* Reset parser global state across compilations, then compile */
  vix_reset_parser_state();
  int ret = run_compiler_pipeline(&opts, preprocessed_file);

  fclose(preprocessed_file);
  fclose(input_file);
  return ret;
}

/* ==================================================================
 * Usage / Help
 * ================================================================== */
static void print_usage(const char *prog) {
  fprintf(stderr, "OVERVIEW: Vix Compiler\n\n");
  fprintf(stderr, "USAGE: %s [options] <input.vix>\n", prog);
  fprintf(stderr, "       %s file1.o file2.o ... -o <output>\n\n", prog);
  fprintf(stderr, "OPTIONS:\n");
  fprintf(stderr, "  -o <file>              Write output to <file>\n");
  fprintf(stderr, "  -S [file]              Emit assembly to <file> "
                  "(default: <input>.s)\n");
  fprintf(stderr, "  -obj [file]            Emit object file to <file> "
                  "(default: <input>.o)\n");
  fprintf(stderr, "  -ll [file]             Emit LLVM IR to <file> (default: "
                  "<input>.ll)\n");
  fprintf(stderr, "  -llvm                  Print LLVM IR to stdout\n");
  fprintf(stderr, "  -ast                   Print AST to stdout\n");
  fprintf(stderr,
          "  -opt=lN                Set optimization level (N = 0..3)\n");
  fprintf(stderr,
          "  --target=<triple>      Set codegen/link target triple\n");
  fprintf(stderr,
          "  -static                Static linking (default: dynamic)\n");
  fprintf(stderr, "  -L <path>              Add library search path\n");
  fprintf(stderr, "  -l <lib>               Link with library\n");
  fprintf(stderr, "  --check                Syntax & type check only\n");
  fprintf(stderr, "  --time                 Show phase timing breakdown\n");
  fprintf(stderr, "  --debug                Enable debug output\n");
  fprintf(stderr,
          "  -v, --version          Display compiler version information\n");
  fprintf(stderr, "  -h, --help             Display this help message\n");
}

/* ==================================================================
 * CLI argument parsing
 * ================================================================== */
static int parse_args(VixOptions *opts, int argc, char **argv) {
  for (int i = 1; i < argc; i++) {
    if (strncmp(argv[i], "--target=", 9) == 0) {
      opts->target = argv[i] + 9;
    } else if (strcmp(argv[i], "--target") == 0) {
      if (i + 1 < argc) { opts->target = argv[++i]; }
      else { fprintf(stderr, "Error: --target requires a triple\n"); return 0; }
    } else if (strcmp(argv[i], "-o") == 0) {
      if (i + 1 < argc) { opts->out_f = argv[++i]; opts->save_c = 1; }
      else { fprintf(stderr, "Error: -o requires a filename\n"); return 0; }
    } else if (strcmp(argv[i], "-v") == 0 || strcmp(argv[i], "--version") == 0 ||
               strcmp(argv[i], "-ver") == 0) {
      printf("Vix Compiler 0.5.0 Copyright(c) 2025-2026 LLVM : 22.1.2(8)\n");
      exit(0);
    } else if (strcmp(argv[i], "-llvm") == 0) {
      opts->out_llvm = 1;
    } else if (strcmp(argv[i], "-obj") == 0) {
      opts->gen_obj = 1; opts->gen_llvm = 1;
      if (i + 1 < argc && argv[i + 1][0] != '-') opts->obj_f = argv[++i];
    } else if (strcmp(argv[i], "-S") == 0 || strcmp(argv[i], "-s") == 0) {
      opts->gen_asm = 1; opts->gen_llvm = 1;
      if (i + 1 < argc && argv[i + 1][0] != '-') opts->asm_f = argv[++i];
    } else if (strcmp(argv[i], "-ll") == 0) {
      if (i + 1 < argc && argv[i + 1][0] != '-') { opts->llvm_f = argv[++i]; opts->gen_llvm = 1; }
      else { opts->out_llvm = 1; }
    } else if (strcmp(argv[i], "-ast") == 0 || strcmp(argv[i], "--ast") == 0) {
      opts->out_ast = 1;
    } else if (strcmp(argv[i], "--debug") == 0) {
      opts->dbg = 1;
    } else if (strcmp(argv[i], "--check") == 0) {
      opts->check_only = 1;
    } else if (strcmp(argv[i], "--time") == 0) {
      opts->show_time = 1;
    } else if (strncmp(argv[i], "-opt=l", 6) == 0) {
      int lvl = argv[i][6] - '0';
      if (lvl < 0 || lvl > 3 || argv[i][7] != '\0') {
        fprintf(stderr, "Error: -opt=lN requires N in 0..3 (got %s)\n", argv[i]);
        return 0;
      }
      opts->opt_level = lvl;
    } else if (strcmp(argv[i], "-static") == 0) {
      opts->static_link = 1;
    } else if (strcmp(argv[i], "-L") == 0) {
      if (i + 1 < argc && opts->lib_path_count < MAX_LIB_PATHS) opts->lib_paths[opts->lib_path_count++] = argv[++i];
      else { fprintf(stderr, "Error: -L requires a path\n"); return 0; }
    } else if (strcmp(argv[i], "-l") == 0) {
      if (i + 1 < argc && opts->extra_lib_count < MAX_EXTRA_LIBS) opts->extra_libs[opts->extra_lib_count++] = argv[++i];
      else { fprintf(stderr, "Error: -l requires a library name\n"); return 0; }
    } else if (strcmp(argv[i], "-h") == 0 || strcmp(argv[i], "--help") == 0) {
      print_usage(argv[0]);
      exit(0);
    } else if (argv[i][0] == '-' && strcmp(argv[i], "-") != 0) {
      fprintf(stderr, "Unknown option: %s\n", argv[i]);
      return 0;
    } else {
      size_t len = strlen(argv[i]);
      if (len > 2 && strcmp(argv[i] + len - 2, ".o") == 0) {
        if (opts->obj_file_count < MAX_OBJ_FILES) {
          opts->obj_files[opts->obj_file_count++] = argv[i];
          opts->link_mode = 1;
        } else {
          fprintf(stderr, "Error: too many object files (max %d)\n", MAX_OBJ_FILES);
          return 0;
        }
      } else {
        opts->in_f = argv[i];
        opts->is_vic = len > 4 && strcmp(argv[i] + len - 4, ".vic") == 0;
      }
    }
  }
  return 1;
}

/* ==================================================================
 * Linker mode: .o files → executable
 * ================================================================== */
static int run_linker_mode(const VixOptions *opts) {
  if (!opts->out_f) {
    fprintf(stderr, "Error: -o <output> required when linking object files\n");
    return 1;
  }
  const char *eff_t = opts->target;
  int bare = eff_t && (strstr(eff_t, "unknown-none") != NULL);

  const char *ls = NULL;
  if (bare) {
    ls = "linker.ld";
    if (!vix_file_readable(ls) && vix_file_readable("src/linker.ld"))
      ls = "src/linker.ld";
  }

  const char *link_err = NULL;
  VixLinkOptions link_opts = {
    .target_triple = eff_t,
    .bare_mode = bare,
    .linker_script = ls,
    .static_link = bare || opts->static_link,
    .entry_point = bare ? "_start" : NULL,
    .libc_dir = find_bundled_libc(),
    .lib_paths = opts->lib_path_count > 0 ? (const char **)opts->lib_paths : NULL,
    .lib_path_count = opts->lib_path_count,
    .extra_libs = opts->extra_lib_count > 0 ? (const char **)opts->extra_libs : NULL,
    .extra_lib_count = opts->extra_lib_count,
  };

  if (!vix_link_multi((const char **)opts->obj_files, opts->obj_file_count,
                      opts->out_f, &link_opts, &link_err)) {
    fprintf(stderr, "Error: Failed to link object files");
    if (link_err && link_err[0] != '\0') fprintf(stderr, ":\n%s", link_err);
    fprintf(stderr, "\n");
    return 1;
  }
  return 0;
}

/* ==================================================================
 * Compiler pipeline: parse → semantic → typeck → ownership → codegen
 * ================================================================== */
static int run_compiler_pipeline(const VixOptions *opts, FILE *input_file) {
  int result = 1; /* default: failure */
  struct timespec t_start, t_file_ts, t_parse_ts, t_sema_ts, t_codegen_ts;

  /* Derive effective target triple */
  const char *eff_t = opts->target;
  if (!eff_t && (opts->no_std || opts->no_main))
    eff_t = "x86_64-unknown-none";

#ifndef VIXC_FRONTEND_ONLY
  llvm_set_target_triple(eff_t);
#endif

  int bare = eff_t && (strstr(eff_t, "unknown-none") != NULL);
  if (opts->no_std || opts->no_main) bare = 1;

  /* Default output filename */
  int free_out_f = 0;
  if (!opts->gen_llvm && !opts->out_ast && !opts->out_llvm &&
      !opts->out_f && !opts->save_c && opts->in_f) {
    char *dot = strrchr(opts->in_f, '.');
    size_t base_len = dot ? (size_t)(dot - opts->in_f) : strlen(opts->in_f);
    char *def_out = malloc(base_len + 1);
    if (def_out) {
      strncpy(def_out, opts->in_f, base_len);
      def_out[base_len] = '\0';
#ifdef _WIN32
      char *tmp = realloc(def_out, base_len + 5);
      if (tmp) { def_out = tmp; strcat(def_out, ".exe"); }
#endif
      /* Cast away const — opts->out_f points to malloc'd memory we own */
      *(char **)&opts->out_f = def_out;
      *(int *)&opts->save_c = 1;
      free_out_f = 1;
    }
  }

  /* LLVM IR output path */
  char llvm_filename[2048];
  const char *llvm_f = opts->llvm_f;
  if ((opts->save_c || opts->gen_obj || opts->gen_asm) && !llvm_f) {
    const char *base = opts->out_f ? opts->out_f : opts->in_f;
    char *dot = strrchr(base, '.');
    if (dot) {
      size_t len = dot - base;
      snprintf(llvm_filename, sizeof(llvm_filename), "%.*s.ll", (int)len, base);
    } else {
      snprintf(llvm_filename, sizeof(llvm_filename), "%s.ll", base);
    }
    llvm_f = llvm_filename;
  }

  /* ── Phase 0: Timing setup ── */
  if (opts->show_time)
    clock_gettime(CLOCK_MONOTONIC, &t_start);

  current_input_filename = opts->in_f;
  load_source_file(opts->in_f);
  set_location_with_column(opts->in_f, 1, 1);
  yyin = input_file;

  if (opts->show_time)
    clock_gettime(CLOCK_MONOTONIC, &t_file_ts);

  /* ── Phase 1: Parse ── */
  result = yyparse();
  if (result == 0 && root && !inline_imports(root))
    result = 1;

  if (opts->show_time)
    clock_gettime(CLOCK_MONOTONIC, &t_parse_ts);

  /* ── Phase 2: Semantic analysis → Type check → Ownership → Unused vars ── */
  if (result == 0) {
    result = 1; /* default: failure until we reach the success path below */
    int errs = check_undefined_symbols(root);
    if (errs > 0) {
      fprintf(stderr, "Error: Found %d semantic error(s)\n", errs);
      goto cleanup;
    }

    if (typecheck_program(root) != 0) {
      fprintf(stderr, "Compilation failed with type errors\n");
      goto cleanup;
    }
    if (ownership_check_program(root) != 0) {
      fprintf(stderr, "Compilation failed with ownership errors\n");
      goto cleanup;
    }

    {
      SymbolTable *g_tbl = create_symbol_table(NULL);
      int uvars = check_unused_variables(root, g_tbl);
      destroy_symbol_table(g_tbl);
      (void)uvars;
    }

    if (get_error_count() > 0) {
      fprintf(stderr, "Compilation failed with %d error(s)\n", get_error_count());
      goto cleanup;
    }

    if (opts->show_time)
      clock_gettime(CLOCK_MONOTONIC, &t_sema_ts);
    if (opts->show_time)
      t_codegen_ts = t_sema_ts;

    /* ── Phase 3: Check-only mode ── */
    if (opts->check_only) {
      print_error_summary();
      result = (get_error_count() > 0) ? 1 : 0;
      goto cleanup;
    }

    /* ── Phase 4: Codegen ── */
#ifndef VIXC_FRONTEND_ONLY
    if (opts->gen_llvm || opts->save_c) {
      if (!llvm_f) {
        fprintf(stderr, "Error: could not determine LLVM IR output path\n");
        goto cleanup;
      }

      /* Extension check: ensure .ll suffix */
      char llvm_buf[2048];
      if (strstr(llvm_f, ".ll") == NULL) {
        int written = snprintf(llvm_buf, sizeof(llvm_buf), "%s.ll", llvm_f);
        if (written < 0 || (size_t)written >= sizeof(llvm_buf)) {
          fprintf(stderr, "Error: LLVM IR output path is too long\n");
          goto cleanup;
        }
        llvm_f = llvm_buf;
      }

      FILE *llvm_file = fopen(llvm_f, "w");
      if (!llvm_file) {
        fprintf(stderr, "Error: Cannot open LLVM IR file %s for writing\n", llvm_f);
        goto cleanup;
      }
      llvm_emit_from_ast(root, llvm_file);
      fclose(llvm_file);

      if (opts->show_time)
        clock_gettime(CLOCK_MONOTONIC, &t_codegen_ts);

      if (get_error_count() > 0) {
        fprintf(stderr, "Compilation failed with %d error(s)\n", get_error_count());
        remove(llvm_f);
        goto cleanup;
      }

      /* ── .o file generation ── */
      if (opts->gen_obj) {
        char oname[2048];
        const char *fobj = opts->obj_f;
        if (!fobj) {
          char *dot = strrchr(opts->in_f, '.');
          if (dot) {
            size_t len = dot - opts->in_f;
            snprintf(oname, sizeof(oname), "%.*s.o", (int)len, opts->in_f);
          } else {
            snprintf(oname, sizeof(oname), "%s.o", opts->in_f);
          }
          fobj = oname;
        } else if (strstr(fobj, ".o") == NULL) {
          snprintf(oname, sizeof(oname), "%s.o", fobj);
          fobj = oname;
        }

        const char *llc_err = NULL;
        if (!llc_compile_to_object(llvm_f, fobj, eff_t ? eff_t : "", bare,
                                   opts->opt_level, &llc_err)) {
          fprintf(stderr, "Error: Failed to compile LLVM IR to object file via Llc");
          if (llc_err && llc_err[0] != '\0') fprintf(stderr, ": %s", llc_err);
          fprintf(stderr, "\n");
          goto cleanup;
        }

        if (!opts->save_c) {
          remove(llvm_f);
          result = 0;
          goto done;
        }
      }

      /* ── Assembly (.s) generation ── */
      if (opts->gen_asm) {
        char oname[2048];
        const char *fasm = opts->asm_f;
        if (!fasm) {
          char *dot = strrchr(opts->in_f, '.');
          if (dot) {
            size_t len = dot - opts->in_f;
            snprintf(oname, sizeof(oname), "%.*s.s", (int)len, opts->in_f);
          } else {
            snprintf(oname, sizeof(oname), "%s.s", opts->in_f);
          }
          fasm = oname;
        } else if (strstr(fasm, ".s") == NULL) {
          snprintf(oname, sizeof(oname), "%s.s", fasm);
          fasm = oname;
        }

        const char *llc_err = NULL;
        if (!llc_compile_to_assembly(llvm_f, fasm, eff_t ? eff_t : "", bare,
                                     opts->opt_level, &llc_err)) {
          fprintf(stderr, "Error: Failed to compile LLVM IR to assembly via Llc");
          if (llc_err && llc_err[0] != '\0') fprintf(stderr, ": %s", llc_err);
          fprintf(stderr, "\n");
          goto cleanup;
        }
        remove(llvm_f);
        result = 0;
        goto done;
      }

      /* ── Link to executable ── */
      if (opts->out_f && opts->save_c) {
        const char *ls = NULL;
        if (bare) {
          ls = "linker.ld";
          if (!vix_file_readable(ls) && vix_file_readable("src/linker.ld"))
            ls = "src/linker.ld";
        }

        char obj_file[2048];
        {
          char *dot = strrchr(llvm_f, '.');
          if (dot) {
            size_t len = dot - llvm_f;
            snprintf(obj_file, sizeof(obj_file), "%.*s.o", (int)len, llvm_f);
          } else {
            int written = snprintf(obj_file, sizeof(obj_file), "%s.o", llvm_f);
            if (written < 0 || (size_t)written >= sizeof(obj_file)) {
              fprintf(stderr, "Error: object output path is too long\n");
              remove(llvm_f);
              goto cleanup;
            }
          }
        }

        const char *llc_err = NULL;
        if (!llc_compile_to_object(llvm_f, obj_file, eff_t ? eff_t : "",
                                   bare, opts->opt_level, &llc_err)) {
          fprintf(stderr, "Error: Failed to compile LLVM IR to object file via Llc");
          if (llc_err && llc_err[0] != '\0') fprintf(stderr, ": %s", llc_err);
          fprintf(stderr, "\n");
          remove(llvm_f);
          goto cleanup;
        }

        const char *link_err = NULL;
        VixLinkOptions link_opts = {
          .target_triple = eff_t,
          .bare_mode = bare,
          .linker_script = ls,
          .static_link = bare || opts->static_link,
          .entry_point = bare ? "_start" : NULL,
          .libc_dir = find_bundled_libc(),
          .lib_paths = opts->lib_path_count > 0 ? (const char **)opts->lib_paths : NULL,
          .lib_path_count = opts->lib_path_count,
          .extra_libs = opts->extra_lib_count > 0 ? (const char **)opts->extra_libs : NULL,
          .extra_lib_count = opts->extra_lib_count,
        };
        if (!vix_link(obj_file, opts->out_f, &link_opts, &link_err)) {
          fprintf(stderr, "Error: Failed to link object file to executable");
          if (link_err && link_err[0] != '\0') fprintf(stderr, ":\n%s", link_err);
          fprintf(stderr, "\n");
          remove(llvm_f);
          remove(obj_file);
          goto cleanup;
        }
        remove(llvm_f);
        remove(obj_file);
        result = 0;
        goto done;
      }

      result = 0;
      goto done;
    }
#endif

    if (opts->out_ast) {
      printf("===========================AST=======================\n");
      print_ast(root, 0);
      printf("===================================================\n");
      result = 0;
      goto done;
    }

    if (opts->out_llvm) {
#ifndef VIXC_FRONTEND_ONLY
      printf("=========================LLVM IR===================\n");
      llvm_emit_from_ast(root, stdout);
      printf("===================================================\n");
#else
      fprintf(stderr, "Error: LLVM output not available (frontend-only build)\n");
#endif
      result = 0;
      goto done;
    }

#ifndef VIXC_FRONTEND_ONLY
    result = 0;
    goto done;
#endif
  }

  if (get_error_count() == 0 && result != 0) {
    const char *fname = current_input_filename ? current_input_filename : "unknown";
    report_syntax_error_with_location("parsing failed due to syntax errors", fname, 1);
  }
  result = 1;

/* ── Unified cleanup ── */
cleanup:
done:
  if (opts->show_time && result == 0)
    print_timing_table(t_start, t_file_ts, t_parse_ts, t_sema_ts, t_codegen_ts);
  print_error_summary();
  if (root) { free_ast(root); root = NULL; }
  cleanup_error_handler();
  if (free_out_f && opts->out_f) { free(opts->out_f); }
  return result;
}

/* ==================================================================
 * Timing display
 * ================================================================== */
static void print_timing_table(struct timespec t_start, struct timespec t_file,
                               struct timespec t_parse, struct timespec t_sema,
                               struct timespec t_codegen) {
  double ms_file = (t_file.tv_sec - t_start.tv_sec) * 1000.0 +
                   (t_file.tv_nsec - t_start.tv_nsec) / 1e6;
  double ms_parse = (t_parse.tv_sec - t_file.tv_sec) * 1000.0 +
                    (t_parse.tv_nsec - t_file.tv_nsec) / 1e6;
  double ms_sema = (t_sema.tv_sec - t_parse.tv_sec) * 1000.0 +
                   (t_sema.tv_nsec - t_parse.tv_nsec) / 1e6;
  double ms_codegen = (t_codegen.tv_sec - t_sema.tv_sec) * 1000.0 +
                      (t_codegen.tv_nsec - t_sema.tv_nsec) / 1e6;
  double ms_total = ms_file + ms_parse + ms_sema + ms_codegen;

  fprintf(stderr,
          "\033[36m── Phase Timings ──────────────────────────────\033[0m\n");
  fprintf(stderr, "  File I/O    %9.2f ms  %5.1f%%\n", ms_file,
          ms_file / ms_total * 100);
  fprintf(stderr, "  Parse       %9.2f ms  %5.1f%%\n", ms_parse,
          ms_parse / ms_total * 100);
  fprintf(stderr, "  Semantic    %9.2f ms  %5.1f%%\n", ms_sema,
          ms_sema / ms_total * 100);
  fprintf(stderr, "  Codegen     %9.2f ms  %5.1f%%\n", ms_codegen,
          ms_codegen / ms_total * 100);
  fprintf(stderr,
          "\033[36m  ────────────────────────────────────────────\033[0m\n");
  fprintf(stderr, "  Total       %9.2f ms\n", ms_total);
}

/* ==================================================================
 * Find bundled libc directory (relative to compiler executable)
 * ================================================================== */
static const char *find_bundled_libc(void) {
  static char libc_path[4096];
  char exe_dir[4096];

#ifdef _WIN32
  DWORD len = GetModuleFileNameA(NULL, exe_dir, sizeof(exe_dir));
  if (len == 0 || len >= sizeof(exe_dir)) return NULL;
  char *last_sep = strrchr(exe_dir, '\\');
  if (!last_sep) last_sep = strrchr(exe_dir, '/');
  if (last_sep) *last_sep = '\0';
#else
  ssize_t len = readlink("/proc/self/exe", exe_dir, sizeof(exe_dir) - 1);
  if (len <= 0) return NULL;
  exe_dir[len] = '\0';
  char *last_sep = strrchr(exe_dir, '/');
  if (last_sep) *last_sep = '\0';
#endif

  static const char *const suffixes[] = {
#ifdef _WIN32
      "\\..\\libc", "\\libc",
#else
      "/../libc", "/libc",
#endif
      NULL};

  for (const char *const *s = suffixes; *s; ++s) {
    snprintf(libc_path, sizeof(libc_path), "%s%s", exe_dir, *s);
#ifdef _WIN32
    DWORD attr = GetFileAttributesA(libc_path);
    if (attr != INVALID_FILE_ATTRIBUTES && (attr & FILE_ATTRIBUTE_DIRECTORY))
      return libc_path;
#else
    struct stat st;
    if (stat(libc_path, &st) == 0 && S_ISDIR(st.st_mode))
      return libc_path;
#endif
  }
  return NULL;
}
