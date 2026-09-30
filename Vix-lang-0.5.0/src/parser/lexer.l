%{
#define _POSIX_C_SOURCE 200809L
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "parser.tab.h"
#include "../include/ast.h"
#include "../include/compiler.h"
extern const char* current_input_filename;
extern YYSTYPE yylval;

int yycolumn = 1;

/* Automatic Semicolon Insertion (ASI) state */
static int asi_prev_token = 0;
static int asi_insert_semi = 0;

/* Check if a token type can end a statement (for ASI) */
static int asi_token_can_end_statement(int token) {
    switch (token) {
        case IDENTIFIER:
        case NUMBER_INT:
        case NUMBER_FLOAT:
        case CHAR_LITERAL:
        case BOOL_LITERAL:
        case STRING:
        case RPAREN:
        case RBRACKET:
        case RBRACE:
        case RETURN:
        case BREAK:
        case CONTINUE:
            return 1;
        default:
            return 0;
    }
}

/* Check if a token type can start a new statement (for ASI) */
static int asi_token_can_start_statement(int token) {
    switch (token) {
        case IDENTIFIER:
        case NUMBER_INT:
        case NUMBER_FLOAT:
        case CHAR_LITERAL:
        case BOOL_LITERAL:
        case STRING:
        case LET:
        case IF:
        case ELIF:
        case ELSE:
        case WHILE:
        case FOR:
        case MATCH:
        case RETURN:
        case BREAK:
        case CONTINUE:
        case PRINT:
        case FN:
        case PUB:
        case EXTERN:
        case IMPORT:
        case STRUCT:
        case TYPE_KW:
        case IMPL:
        case LBRACE:
        case LBRACKET:
        case BANG:
        case MINUS:
        case AT:
        case AMPERSAND:
        case NIL:
            return 1;
        default:
            return 0;
    }
}

char* my_strndup(const char* str, size_t n) {
    size_t len = strlen(str);
    if (n < len) {
        len = n;
    }
    
    char* result = malloc(len + 1);
    if (result) {
        memcpy(result, str, len);
        result[len] = '\0';
    }
    return result;
}
char* process_escape_sequences(const char* input, size_t input_len) {
    char* result = malloc(input_len + 1);
    size_t result_idx = 0;
    for (size_t i = 0; i < input_len; i++) {
        if (input[i] == '\\' && i + 1 < input_len) {
            switch (input[i + 1]) {
                case 'n':
                    result[result_idx++] = '\n';
                    i++;
                    break;
                case 't':
                    result[result_idx++] = '\t';
                    i++;
                    break;
                case 'r':
                    result[result_idx++] = '\r';
                    i++;
                    break;
                case '\\':
                    result[result_idx++] = '\\';
                    i++;
                    break;
                case '"':
                    result[result_idx++] = '"';
                    i++;
                    break;
                case '\'':
                    result[result_idx++] = '\'';
                    i++;
                    break;
                case '0':
                    // Check for octal escape sequence like \033
                    if (i + 3 < input_len && 
                        input[i + 2] >= '0' && input[i + 2] <= '7' &&
                        input[i + 3] >= '0' && input[i + 3] <= '7') {
                        int octal_val = (input[i + 1] - '0') * 64 + 
                                        (input[i + 2] - '0') * 8 +
                                        (input[i + 3] - '0');
                        result[result_idx++] = (char)octal_val;
                        i += 3;
                    } else {
                        result[result_idx++] = '\0';
                        i++;
                    }
                    break;
                default:
                    result[result_idx++] = input[i];
                    break;
            }
        } else {
            result[result_idx++] = input[i];
        }
    }
    
    result[result_idx] = '\0';
    return result;
}

static int decode_char_literal(const char* text, size_t len, unsigned char* out) {
    if (!text || len < 3 || text[0] != '\'' || text[len - 1] != '\'') {
        return 0;
    }

    const char* body = text + 1;
    size_t body_len = len - 2;
    if (body_len == 0) return 0;

    if (body[0] != '\\') {
        if (body_len != 1) return 0;
        *out = (unsigned char)body[0];
        return 1;
    }

    if (body_len != 2) return 0;
    switch (body[1]) {
        case 'n': *out = (unsigned char)'\n'; return 1;
        case 't': *out = (unsigned char)'\t'; return 1;
        case 'r': *out = (unsigned char)'\r'; return 1;
        case '0': *out = (unsigned char)'\0'; return 1;
        case '\\': *out = (unsigned char)'\\'; return 1;
        case '\'': *out = (unsigned char)'\''; return 1;
        case '"': *out = (unsigned char)'"'; return 1;
        default: return 0;
    }
}
%}

%option noyywrap
%option yylineno
%option nounput
%option noinput
%{
#define UPDATE_COLUMN() \
    do { \
        for (int i = 0; yytext[i] != '\0'; i++) { \
            if (yytext[i] == '\n') { \
                yycolumn = 1; \
            } else if (yytext[i] == '\t') { \
                yycolumn = ((yycolumn - 1) / 8 + 1) * 8 + 1; \
            } else { \
                yycolumn++; \
            } \
        } \
    } while(0)

#define GET_FIRST_COLUMN() (yycolumn - yyleng)

#define SET_TOKEN_LOCATION() \
    do { \
        int col = GET_FIRST_COLUMN(); \
        yylloc.first_line = yylineno; \
        yylloc.first_column = col; \
        yylloc.last_line = yylineno; \
        yylloc.last_column = col + yyleng - 1; \
    } while(0)

#define RETURN_TOKEN(tok) \
    do { \
        asi_insert_semi = 0; \
        if (asi_token_can_end_statement(asi_prev_token) && asi_token_can_start_statement(tok)) { \
            asi_insert_semi = 1; \
        } \
        asi_prev_token = tok; \
        return tok; \
    } while(0)

#define RETURN_TOKEN_SEMI(tok, is_semi) \
    do { \
        asi_insert_semi = 0; \
        asi_prev_token = tok; \
        if (is_semi) asi_insert_semi = 0; \
        return tok; \
    } while(0)
%}

%%

[ \t\r]              { 
                       for (int i = 0; yytext[i] != '\0'; i++) {
                           if (yytext[i] == '\t') {
                               yycolumn = ((yycolumn - 1) / 8 + 1) * 8 + 1;
                           } else {
                               yycolumn++;
                           }
                       }
                       } 

\n                   { 
                       yycolumn = 1;
                       if (asi_token_can_end_statement(asi_prev_token)) {
                           asi_insert_semi = 1;
                       }
                       } 

"#"[^\n]* { UPDATE_COLUMN(); }
"//"[^\n]*         { UPDATE_COLUMN(); }

"print"             { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(PRINT); }
"input"             { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(INPUT); }
"string"            { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(TYPE_STR); }
"return"            { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(RETURN); }
"fn"                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(FN); }
"u32"               { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(TYPE_I32); }
"i64"               { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(TYPE_I64); }
"u64"               { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(TYPE_I64); }
"i8"                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(TYPE_I8); }
"u8"                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(TYPE_I8); }
"f32"               { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(TYPE_F32); }
"f64"               { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(TYPE_F64); }
"usize"             { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(TYPE_I64); }
"extern"            { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(EXTERN); }
"const"             { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); /* const removed from user syntax */ }
"let"               { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(LET); }
"mut"               { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(MUT); }
"ref"               { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(REF_KW); }
"->"                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(ARROW); }
"i32"               { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(TYPE_I32); }
"void"              { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(TYPE_VOID); }
"nil"               { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(NIL); }
"if"                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(IF); }
"elif"              { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(ELIF); }
"else"              { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(ELSE); }
"while"             { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(WHILE); }
"break"             { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(BREAK); }
"continue"          { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(CONTINUE); }
"for"               { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(FOR); }
"in"                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(IN); }
"global"           { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(IDENTIFIER); }
"struct"           { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(STRUCT); }
"and"              { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(AND); }
"or"               { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(OR); }
"import"           { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(IMPORT); }
"pub"               { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(PUB); }
"impl"              { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(IMPL); }
"type"              { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(TYPE_KW); }
"match"             { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(MATCH); }
"true"              { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.num_int = 1; RETURN_TOKEN(BOOL_LITERAL); }
"false"             { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.num_int = 0; RETURN_TOKEN(BOOL_LITERAL); }

"="                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(ASSIGN); }
"+="                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(PLUS_ASSIGN); }
"-="                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); yylval.str = strdup(yytext); RETURN_TOKEN(MINUS_ASSIGN); }
"*="                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(MULTIPLY_ASSIGN); }
"/="                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(DIVIDE_ASSIGN); }
"%="                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(MODULO_ASSIGN); }
"+"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(PLUS); }
"-"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(MINUS); }
"*"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(MULTIPLY); }
"/"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(DIVIDE); }
"%"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(MODULO); }
"**"                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(POWER); }
"=="                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(EQ); }
"!="                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(NE); }
"!"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(BANG); }
"<"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(LT); }
"<="                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(LE); }
">"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(GT); }
">="                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(GE); }
"|>"                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(PIPE_GT); }

"("                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(LPAREN); }
")"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(RPAREN); }
"{"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(LBRACE); }
"}"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(RBRACE); }
"["                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(LBRACKET); }
"]"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(RBRACKET); }
"@"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(AT); }
"&"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(AMPERSAND); }

";"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); asi_prev_token = SEMICOLON; asi_insert_semi = 0; return SEMICOLON; }
","                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(COMMA); }
"..."               { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(DOTDOTDOT); }
".."                { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(DOTDOT); }
"."                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(DOT); }
":"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(COLON); }
"|"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(PIPE); }
"?"                 { UPDATE_COLUMN(); SET_TOKEN_LOCATION(); RETURN_TOKEN(QUESTION); }

[a-zA-Z_][a-zA-Z0-9_]*!?  {
     UPDATE_COLUMN();
     int col = GET_FIRST_COLUMN();
                        yylval.str = strdup(yytext);
                        yylloc.first_line = yylineno;
                        yylloc.first_column = col;
                        yylloc.last_line = yylineno;
                        yylloc.last_column = col + yyleng - 1;
                        RETURN_TOKEN(IDENTIFIER);
                      }

0[xX][0-9a-fA-F]+   {
                                                int col = GET_FIRST_COLUMN();
                                                yylval.num_int = strtoll(yytext, NULL, 16);
                                                yylloc.first_line = yylineno;
                                                yylloc.first_column = col;
                                                yylloc.last_line = yylineno;
                                                yylloc.last_column = col + yyleng - 1;
                                                RETURN_TOKEN(NUMBER_INT);
                                            }

[0-9]+              {
                        int col = GET_FIRST_COLUMN();
                        yylval.num_int = atoll(yytext);
                        yylloc.first_line = yylineno;
                        yylloc.first_column = col;
                        yylloc.last_line = yylineno;
                        yylloc.last_column = col + yyleng - 1;
                        RETURN_TOKEN(NUMBER_INT);
                      }

[0-9]+"."[0-9]+      {
                        int col = GET_FIRST_COLUMN();
                        yylval.num_float = atof(yytext);
                        yylloc.first_line = yylineno;
                        yylloc.first_column = col;
                        yylloc.last_line = yylineno;
                        yylloc.last_column = col + yyleng - 1;
                        RETURN_TOKEN(NUMBER_FLOAT);
                      }

\"([^\"\\]|\\[\"\\nrt0\\'])*\"        {
                        int col = GET_FIRST_COLUMN();
                        char* raw_str = my_strndup(yytext + 1, yyleng - 2);
                        yylval.str = process_escape_sequences(raw_str, strlen(raw_str));
                        free(raw_str);
                        yylloc.first_line = yylineno;
                        yylloc.first_column = col;
                        yylloc.last_line = yylineno;
                        yylloc.last_column = col + yyleng - 1;
                        RETURN_TOKEN(STRING);
                      }

\'([^\\\'\n]|\\[nrt0\\\'\"])\'        {
                        int col = GET_FIRST_COLUMN();
                        unsigned char c = 0;
                        if (!decode_char_literal(yytext, (size_t)yyleng, &c)) {
                            report_lexical_error_with_location("Invalid char literal", current_input_filename ? current_input_filename : "unknown", yylineno);
                            return ERROR;
                        }
                        yylval.num_int = (long long)c;
                        yylloc.first_line = yylineno;
                        yylloc.first_column = col;
                        yylloc.last_line = yylineno;
                        yylloc.last_column = col + yyleng - 1;
                        RETURN_TOKEN(CHAR_LITERAL);
                      }

"/"[*]([^*]|[*]+[^*/])*[*]+"/"  { UPDATE_COLUMN(); } 

.                   {
                        char error_msg[256];
                        snprintf(error_msg, sizeof(error_msg), "Unknown: %c", yytext[0]);
                        report_lexical_error_with_location(error_msg, current_input_filename ? current_input_filename : "unknown", yylineno);
                        return ERROR;
                    }

%%

/*
 *wocao tmd 这个lexer也是真tm nb啊，我tm都不知道我是怎么写出来的
 */
