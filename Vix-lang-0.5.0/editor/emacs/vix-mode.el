;;; vix-mode.el --- Major mode for the Vix programming language  -*- lexical-binding: t; -*-

;; Copyright (C) 2026

;; Author: Vix contributors
;; Keywords: languages, vix
;; Version: 0.1
;; Homepage: https://github.com/anomalyco/Vix-lang

;; This file is not part of GNU Emacs.

;;; Commentary:

;; A simple major mode for editing Vix (https://github.com/anomalyco/Vix-lang)
;; source files.  Provides syntax highlighting, basic indentation,
;; and navigation support.

;;; Code:

(defgroup vix-mode nil
  "Major mode for editing Vix source code."
  :prefix "vix-"
  :group 'languages)

(defvar vix-mode-syntax-table
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?_ "w" st)
    (modify-syntax-entry ?/  ". 124" st)
    (modify-syntax-entry ?*  ". 23" st)
    (modify-syntax-entry ?\n "> b" st)
    (modify-syntax-entry ?\" "\"" st)
    (modify-syntax-entry ?\\ "\\" st)
    st)
  "Syntax table for `vix-mode'.")

(defvar vix-keywords
  '("fn" "return" "let" "mut" "if" "elif" "else"
    "while" "for" "in" "match" "struct" "type" "pub"
    "import" "extern" "break" "continue" "and" "or"
    "print" "impl" "as")
  "Vix keyword strings.")

(defvar vix-types
  '("i8" "i16" "i32" "i64" "u8" "u16" "u32" "u64"
    "f32" "f64" "string" "void" "bool" "usize" "isize"
    "nil" "ptr")
  "Vix type strings.")

(defvar vix-builtins
  '("true" "false" "self")
  "Vix builtin constants.")

(defvar vix-ident-re "[a-zA-Z_][a-zA-Z0-9_]*"
  "Regexp matching a Vix identifier.")

(defvar vix-font-lock-keywords
  (let* ((x (lambda (lst) (regexp-opt lst 'symbols)))
         (kw-re (funcall x vix-keywords))
         (type-re (funcall x vix-types))
         (builtin-re (funcall x vix-builtins)))
    `(
      (,(regexp-opt '("TODO" "FIXME" "XXX" "NOTE" "HACK") 'symbols)
       . font-lock-warning-face)

      (,(concat "\\<\\(fn\\)\\>\\s-+\\(" vix-ident-re "\\)")
       (1 font-lock-keyword-face)
       (2 font-lock-function-name-face))

      (,(concat "\\<\\(struct\\|type\\|impl\\)\\>\\s-+\\(" vix-ident-re "\\)")
       (1 font-lock-keyword-face)
       (2 font-lock-type-face))

      (,(concat "\\<\\(import\\)\\>\\s-+\"?\\(" vix-ident-re "\\)\"?")
       (1 font-lock-keyword-face)
       (2 font-lock-string-face))

      ("\\<\\(extern\\)\\>"
       (0 font-lock-keyword-face)
       ("\"C\\|\"rust\"" nil nil (0 font-lock-string-face)))

      (,type-re . font-lock-type-face)
      (,builtin-re . font-lock-builtin-face)
      (,kw-re . font-lock-keyword-face)

      ("\\<[A-Z][a-zA-Z0-9_]*\\>" . font-lock-type-face)

      (":\\[" vix-ident-re "\\(?:,[ \t]*" vix-ident-re "\\)*\\]"
       . font-lock-preprocessor-face)

      ("\\?\\(" vix-ident-re "\\)" . font-lock-type-face)

      ("\\_<0[xX][0-9a-fA-F][0-9a-fA-F_]*\\_>" . font-lock-number-face)
      ("\\_<[0-9][0-9_]*\\(\\.[0-9][0-9_]*\\)?\\_>" . font-lock-number-face)

      ("#\\[[^]]*\\]" . font-lock-preprocessor-face)

      ("+=\\|-=\\|<=\\|>=\\|==\\|!=\\|->\\|=>\\|\\.\\.\\.\\|\\.\\."
       . font-lock-builtin-face)
      ))
  "Font lock keywords for `vix-mode'.")

(defvar vix-mode-map
  (let ((map (make-sparse-keymap)))
    map)
  "Keymap for `vix-mode'.")

(defun vix-indent-line ()
  "Indent current line as Vix code."
  (interactive)
  (let ((savep (point)))
    (back-to-indentation)
    (cond
     ((bobp)
      (indent-line-to 0))
     ((looking-at "}")
      (let ((col))
        (save-excursion
          (backward-sexp)
          (back-to-indentation)
          (setq col (current-column)))
        (indent-line-to col)))
     (t
      (let ((col))
        (save-excursion
          (beginning-of-defun)
          (back-to-indentation)
          (setq col (current-column))
          (unless (zerop col)
            (let ((level 0))
              (save-excursion
                (while (re-search-forward "{" (line-end-position) t)
                  (cl-incf level))
                (while (re-search-forward "}" (line-end-position) t)
                  (cl-decf level)))
              (when (> level 0)
                (cl-incf col (* level vix-indent-offset))))))
        (indent-line-to col))))
    (when (< savep (point))
      (goto-char savep))))

(defcustom vix-indent-offset 4
  "Indent offset for `vix-mode'."
  :type 'integer
  :group 'vix-mode)

(defvar vix-imenu-generic-expression
  `(("Functions" ,(concat "^\\s-*fn\\s-+\\(" vix-ident-re "\\)") 1)
    ("Types" ,(concat "^\\s-*type\\s-+\\(" vix-ident-re "\\)") 1)
    ("Structs" ,(concat "^\\s-*struct\\s-+\\(" vix-ident-re "\\)") 1))
  "Imenu expression for `vix-mode'.")

;;;###autoload
(define-derived-mode vix-mode prog-mode "Vix"
  "Major mode for editing Vix language source files.

\\{vix-mode-map}"
  :syntax-table vix-mode-syntax-table
  (setq-local comment-start "// ")
  (setq-local comment-end "")
  (setq-local comment-start-skip "//+\\s-*")
  (setq-local indent-line-function 'vix-indent-line)
  (setq-local imenu-generic-expression vix-imenu-generic-expression)
  (setq-local font-lock-defaults '(vix-font-lock-keywords nil nil)))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.vix\\'" . vix-mode))

(provide 'vix-mode)

;;; vix-mode.el ends here
