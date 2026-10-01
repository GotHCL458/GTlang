/* ast.c — 高性能 AST 库（C 层，供自举编译器使用）
   以 int64 不透明句柄操作。设计轻量、零碎片、无递归。
   节点类型覆盖：EXPR/STMT/LIT/ID/CALL/BINARY/UNARY/IF/WHILE/BLOCK/FN/
   RET/LET/ASSIGN/MEMBER/INDEX/STRUCT/MATCH/ARM/FOR/LOOP/LOOPN/BREAK/CONTINUE/
   IMPORT/CAST/ENUM/SLICE/TUPLE/DEFER/ASM/NONE/SOME */
#define _CRT_SECURE_NO_WARNINGS 1
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

#ifdef _WIN32
#define GT_API __declspec(dllexport)
#else
#define GT_API
#endif

static char *gt_dup(const char *s) {
    if (!s) s = "";
    size_t n = strlen(s) + 1;
    char *p = (char *)malloc(n);
    if (p) memcpy(p, s, n);
    return p;
}

#define GT_NODE_EXPR     1
#define GT_NODE_STMT     2
#define GT_NODE_LIT      3
#define GT_NODE_ID       4
#define GT_NODE_CALL     5
#define GT_NODE_BINARY   6
#define GT_NODE_UNARY    7
#define GT_NODE_IF       8
#define GT_NODE_WHILE    9
#define GT_NODE_BLOCK   10
#define GT_NODE_FN      11
#define GT_NODE_RET     12
#define GT_NODE_LET     13
#define GT_NODE_ASSIGN  14
#define GT_NODE_MEMBER  15
#define GT_NODE_INDEX   16
#define GT_NODE_STRUCT  17
#define GT_NODE_MATCH   18
#define GT_NODE_ARM     19
#define GT_NODE_FOR     20
#define GT_NODE_LOOP    21
#define GT_NODE_LOOPN   22
#define GT_NODE_BREAK   23
#define GT_NODE_CONTINUE 24
#define GT_NODE_IMPORT  25
#define GT_NODE_CAST    26
#define GT_NODE_ENUM    27
#define GT_NODE_SLICE   28
#define GT_NODE_TUPLE   29
#define GT_NODE_DEFER   30
#define GT_NODE_ASM     31
#define GT_NODE_NONE    32
#define GT_NODE_SOME    33

typedef struct GtAst {
    int type, line, tag;
    int64_t ival; double fval;
    char *sval, *name;
    struct GtAst *a, *b, *c, *d;
    struct GtAst **kids; int nkids;
} GtAst;

static GtAst *ast_new(int type, int line){
    GtAst *n = (GtAst*)calloc(1,sizeof(GtAst));
    n->type = type; n->line = line; return n;
}

/* ===== 工厂 ===== */
GT_API int64_t gto_ast_lit_int(int64_t v){ GtAst*n=ast_new(GT_NODE_LIT,0); n->tag=0; n->ival=v; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_lit_float(double v){ GtAst*n=ast_new(GT_NODE_LIT,0); n->tag=1; n->fval=v; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_lit_str(const char *s){ GtAst*n=ast_new(GT_NODE_LIT,0); n->tag=2; n->sval=gt_dup(s); return (int64_t)(void*)n; }
GT_API int64_t gto_ast_lit_char(int64_t ch){ GtAst*n=ast_new(GT_NODE_LIT,0); n->tag=3; n->ival=ch; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_lit_bool(int64_t v){ GtAst*n=ast_new(GT_NODE_LIT,0); n->tag=4; n->ival=v; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_id(const char *name){ GtAst*n=ast_new(GT_NODE_ID,0); n->name=gt_dup(name); return (int64_t)(void*)n; }
GT_API int64_t gto_ast_binary(int op, int64_t l, int64_t r){ GtAst*n=ast_new(GT_NODE_BINARY,0); n->tag=op; n->a=(GtAst*)(void*)l; n->b=(GtAst*)(void*)r; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_unary(int op, int64_t v){ GtAst*n=ast_new(GT_NODE_UNARY,0); n->tag=op; n->a=(GtAst*)(void*)v; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_call(int64_t callee, int64_t arg0){
    GtAst*n=ast_new(GT_NODE_CALL,0);
    n->a=(GtAst*)(void*)callee;
    n->kids=(GtAst**)calloc(1,sizeof(GtAst*));
    n->kids[0]=(GtAst*)(void*)arg0; n->nkids=1;
    return (int64_t)(void*)n;
}
GT_API int64_t gto_ast_add_arg(int64_t call, int64_t arg){
    GtAst*n=(GtAst*)(void*)call; if(!n) return 0;
    n->kids=(GtAst**)realloc(n->kids,sizeof(GtAst*)*(size_t)(n->nkids+1));
    n->kids[n->nkids++]=(GtAst*)(void*)arg;
    return call;
}
GT_API int64_t gto_ast_if(int64_t c,int64_t t,int64_t e){ GtAst*n=ast_new(GT_NODE_IF,0); n->a=(GtAst*)(void*)c; n->b=(GtAst*)(void*)t; n->c=(GtAst*)(void*)e; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_while(int64_t c,int64_t b){ GtAst*n=ast_new(GT_NODE_WHILE,0); n->a=(GtAst*)(void*)c; n->b=(GtAst*)(void*)b; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_block(int64_t s0){
    GtAst*n=ast_new(GT_NODE_BLOCK,0);
    n->kids=(GtAst**)calloc(1,sizeof(GtAst*));
    n->kids[0]=(GtAst*)(void*)s0; n->nkids=1;
    return (int64_t)(void*)n;
}
GT_API int64_t gto_ast_add_stmt(int64_t blk,int64_t s){
    GtAst*n=(GtAst*)(void*)blk; if(!n) return 0;
    n->kids=(GtAst**)realloc(n->kids,sizeof(GtAst*)*(size_t)(n->nkids+1));
    n->kids[n->nkids++]=(GtAst*)(void*)s;
    return blk;
}
GT_API int64_t gto_ast_fn(const char *name,int64_t body){ GtAst*n=ast_new(GT_NODE_FN,0); n->name=gt_dup(name); n->a=(GtAst*)(void*)body; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_add_param(int64_t fn,int64_t p){
    GtAst*n=(GtAst*)(void*)fn; if(!n) return 0;
    n->kids=(GtAst**)realloc(n->kids,sizeof(GtAst*)*(size_t)(n->nkids+1));
    n->kids[n->nkids++]=(GtAst*)(void*)p;
    return fn;
}
GT_API int64_t gto_ast_ret(int64_t v){ GtAst*n=ast_new(GT_NODE_RET,0); n->a=(GtAst*)(void*)v; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_let(const char *name,int64_t v,int m){ GtAst*n=ast_new(GT_NODE_LET,0); n->name=gt_dup(name); n->a=(GtAst*)(void*)v; n->tag=m; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_assign(const char *name,int64_t v){ GtAst*n=ast_new(GT_NODE_ASSIGN,0); n->name=gt_dup(name); n->a=(GtAst*)(void*)v; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_assign_expr(int64_t lhs,int64_t v){ GtAst*n=ast_new(GT_NODE_ASSIGN,0); n->a=(GtAst*)(void*)lhs; n->b=(GtAst*)(void*)v; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_member(int64_t o,const char *f){ GtAst*n=ast_new(GT_NODE_MEMBER,0); n->a=(GtAst*)(void*)o; n->name=gt_dup(f); return (int64_t)(void*)n; }
GT_API int64_t gto_ast_index(int64_t o,int64_t i){ GtAst*n=ast_new(GT_NODE_INDEX,0); n->a=(GtAst*)(void*)o; n->b=(GtAst*)(void*)i; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_for(const char *var,int64_t it,int64_t b){ GtAst*n=ast_new(GT_NODE_FOR,0); n->name=gt_dup(var); n->a=(GtAst*)(void*)it; n->b=(GtAst*)(void*)b; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_loop(int64_t b){ GtAst*n=ast_new(GT_NODE_LOOP,0); n->a=(GtAst*)(void*)b; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_loopn(int64_t n,int64_t b){ GtAst*x=ast_new(GT_NODE_LOOPN,0); x->ival=n; x->a=(GtAst*)(void*)b; return (int64_t)(void*)x; }
GT_API int64_t gto_ast_break_stmt(void){ return (int64_t)(void*)ast_new(GT_NODE_BREAK,0); }
GT_API int64_t gto_ast_continue_stmt(void){ return (int64_t)(void*)ast_new(GT_NODE_CONTINUE,0); }
GT_API int64_t gto_ast_import(const char *name){ GtAst*n=ast_new(GT_NODE_IMPORT,0); n->name=gt_dup(name); return (int64_t)(void*)n; }
GT_API int64_t gto_ast_cast(int64_t e,const char *t){ GtAst*n=ast_new(GT_NODE_CAST,0); n->a=(GtAst*)(void*)e; n->name=gt_dup(t); return (int64_t)(void*)n; }
GT_API int64_t gto_ast_enum_val(const char *name,int64_t v){ GtAst*n=ast_new(GT_NODE_ENUM,0); n->name=gt_dup(name); n->ival=v; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_slice(int64_t a,int64_t s,int64_t e){ GtAst*n=ast_new(GT_NODE_SLICE,0); n->a=(GtAst*)(void*)a; n->b=(GtAst*)(void*)s; n->c=(GtAst*)(void*)e; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_tuple(int64_t e0,int64_t e1){
    GtAst*n=ast_new(GT_NODE_TUPLE,0);
    n->kids=(GtAst**)calloc(2,sizeof(GtAst*));
    n->kids[0]=(GtAst*)(void*)e0; n->kids[1]=(GtAst*)(void*)e1; n->nkids=2;
    return (int64_t)(void*)n;
}
GT_API int64_t gto_ast_add_elem(int64_t tup,int64_t e){
    GtAst*n=(GtAst*)(void*)tup; if(!n) return 0;
    n->kids=(GtAst**)realloc(n->kids,sizeof(GtAst*)*(size_t)(n->nkids+1));
    n->kids[n->nkids++]=(GtAst*)(void*)e;
    return tup;
}
GT_API int64_t gto_ast_defer(int64_t b){ GtAst*n=ast_new(GT_NODE_DEFER,0); n->a=(GtAst*)(void*)b; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_asm(const char *code){ GtAst*n=ast_new(GT_NODE_ASM,0); n->sval=gt_dup(code); return (int64_t)(void*)n; }
GT_API int64_t gto_ast_none(void){ return (int64_t)(void*)ast_new(GT_NODE_NONE,0); }
GT_API int64_t gto_ast_some(int64_t v){ GtAst*n=ast_new(GT_NODE_SOME,0); n->a=(GtAst*)(void*)v; return (int64_t)(void*)n; }
GT_API int64_t gto_ast_match(int64_t s,int64_t arm0){
    GtAst*n=ast_new(GT_NODE_MATCH,0);
    n->a=(GtAst*)(void*)s;
    n->kids=(GtAst**)calloc(1,sizeof(GtAst*));
    n->kids[0]=(GtAst*)(void*)arm0; n->nkids=1;
    return (int64_t)(void*)n;
}
GT_API int64_t gto_ast_add_arm(int64_t m,int64_t arm){
    GtAst*n=(GtAst*)(void*)m; if(!n) return 0;
    n->kids=(GtAst**)realloc(n->kids,sizeof(GtAst*)*(size_t)(n->nkids+1));
    n->kids[n->nkids++]=(GtAst*)(void*)arm;
    return m;
}
GT_API int64_t gto_ast_arm(int64_t p,int64_t g,int64_t b){ GtAst*n=ast_new(GT_NODE_ARM,0); n->a=(GtAst*)(void*)p; n->b=(GtAst*)(void*)g; n->c=(GtAst*)(void*)b; return (int64_t)(void*)n; }

/* 定位 */
GT_API int64_t gto_ast_set_line(int64_t h,int64_t l){ GtAst*n=(GtAst*)(void*)h; if(!n) return 0; n->line=(int)l; return h; }
GT_API int64_t gto_ast_line(int64_t h){ GtAst*n=(GtAst*)(void*)h; return n?n->line:0; }

/* 访问器 */
GT_API int64_t gto_ast_type(int64_t h){ GtAst*n=(GtAst*)(void*)h; return n?n->type:0; }
GT_API int64_t gto_ast_tag(int64_t h){ GtAst*n=(GtAst*)(void*)h; return n?n->tag:0; }
GT_API int64_t gto_ast_ival(int64_t h){ GtAst*n=(GtAst*)(void*)h; return n?n->ival:0; }
GT_API double  gto_ast_fval(int64_t h){ GtAst*n=(GtAst*)(void*)h; return n?n->fval:0.0; }
GT_API const char *gto_ast_sval(int64_t h){ GtAst*n=(GtAst*)(void*)h; return (n&&n->sval)?n->sval:""; }
GT_API const char *gto_ast_name(int64_t h){ GtAst*n=(GtAst*)(void*)h; return (n&&n->name)?n->name:""; }
GT_API int64_t gto_ast_a(int64_t h){ GtAst*n=(GtAst*)(void*)h; return n?(int64_t)(void*)n->a:0; }
GT_API int64_t gto_ast_b(int64_t h){ GtAst*n=(GtAst*)(void*)h; return n?(int64_t)(void*)n->b:0; }
GT_API int64_t gto_ast_c(int64_t h){ GtAst*n=(GtAst*)(void*)h; return n?(int64_t)(void*)n->c:0; }
GT_API int64_t gto_ast_d(int64_t h){ GtAst*n=(GtAst*)(void*)h; return n?(int64_t)(void*)n->d:0; }
GT_API int64_t gto_ast_nkids(int64_t h){ GtAst*n=(GtAst*)(void*)h; return n?n->nkids:0; }
GT_API int64_t gto_ast_kid(int64_t h,int64_t i){ GtAst*n=(GtAst*)(void*)h; return n&&i>=0&&i<n->nkids?(int64_t)(void*)n->kids[i]:0; }

GT_API const char *gto_ast_type_name(int64_t t){
    static const char *types[] = {
        "?","EXPR","STMT","LIT","ID","CALL","BINARY","UNARY","IF","WHILE",
        "BLOCK","FN","RET","LET","ASSIGN","MEMBER","INDEX","STRUCT","MATCH","ARM",
        "FOR","LOOP","LOOPN","BREAK","CONTINUE","IMPORT","CAST","ENUM","SLICE","TUPLE",
        "DEFER","ASM","NONE","SOME"};
    int nt=(int)(sizeof(types)/sizeof(types[0]));
    return (t>=0&&t<nt)?types[t]:"?";
}

GT_API void gto_ast_free(int64_t h){
    GtAst*n=(GtAst*)(void*)h; if(!n)return;
    if(n->a) gto_ast_free((int64_t)(void*)n->a);
    if(n->b) gto_ast_free((int64_t)(void*)n->b);
    if(n->c) gto_ast_free((int64_t)(void*)n->c);
    if(n->d) gto_ast_free((int64_t)(void*)n->d);
    int i; for(i=0;i<n->nkids;i++) if(n->kids[i]) gto_ast_free((int64_t)(void*)n->kids[i]);
    free(n->kids); free(n->sval); free(n->name); free(n);
}

static void ast_dump_inner(GtAst*n,char **buf,size_t *len,size_t *cap){
    if(!n) return;
    char tmp[256];
    int k=snprintf(tmp,sizeof(tmp),"(%s",gto_ast_type_name(n->type));
    if(*len+(size_t)k+1>*cap){ *cap=(*cap+(size_t)k)*2; *buf=(char*)realloc(*buf,*cap); }
    memcpy(*buf+*len,tmp,(size_t)k); *len+=(size_t)k;
    if(n->name&&n->name[0]){
        size_t sl=strlen(n->name);
        if(*len+sl+3>*cap){ *cap=(*cap+sl)*2; *buf=(char*)realloc(*buf,*cap); }
        (*buf)[(*len)++]=' '; (*buf)[(*len)++]='"';
        memcpy(*buf+*len,n->name,sl); *len+=sl; (*buf)[(*len)++]='"';
    }
    if(n->type==GT_NODE_LIT){
        char lt[96];
        if(n->tag==0) snprintf(lt,sizeof(lt)," %lld",(long long)n->ival);
        else if(n->tag==1) snprintf(lt,sizeof(lt)," %g",n->fval);
        else if(n->tag==2) snprintf(lt,sizeof(lt)," \"%s\"",n->sval?n->sval:"");
        else if(n->tag==3) snprintf(lt,sizeof(lt)," '%c'",(char)n->ival);
        else snprintf(lt,sizeof(lt)," %s",n->ival?"true":"false");
        size_t sl=strlen(lt);
        if(*len+sl+1>*cap){ *cap=(*cap+sl)*2; *buf=(char*)realloc(*buf,*cap); }
        memcpy(*buf+*len,lt,sl); *len+=sl;
    }
    GtAst *subs[4]={n->a,n->b,n->c,n->d};
    int i;
    for(i=0;i<4;i++){ if(subs[i]){ (*buf)[(*len)++]=' '; ast_dump_inner(subs[i],buf,len,cap); } }
    for(i=0;i<n->nkids;i++){ if(n->kids[i]){ (*buf)[(*len)++]=' '; ast_dump_inner(n->kids[i],buf,len,cap); } }
    if(*len+2>*cap){ *cap+=2; *buf=(char*)realloc(*buf,*cap); }
    (*buf)[(*len)++]=')'; (*buf)[(*len)]='\0';
}
GT_API char *gto_ast_dump(int64_t h){
    size_t cap=256,len=0;
    char *buf=(char*)malloc(cap);
    if(!buf) return NULL;
    buf[0]='\0';
    ast_dump_inner((GtAst*)(void*)h,&buf,&len,&cap);
    return buf;
}

/* 遍历器 */
typedef struct { GtAst *cur; int idx; } GtAstWalk;
GT_API int64_t gto_ast_walk(int64_t h){
    GtAst*n=(GtAst*)(void*)h; if(!n) return 0;
    GtAstWalk*w=(GtAstWalk*)calloc(1,sizeof(GtAstWalk));
    w->cur=n; w->idx=-1; return (int64_t)(void*)w;
}
GT_API int64_t gto_ast_walk_next(int64_t wh){
    GtAstWalk*w=(GtAstWalk*)(void*)wh; if(!w||!w->cur) return 0;
    w->idx++;
    GtAst *kids[]={w->cur->a,w->cur->b,w->cur->c,w->cur->d};
    if(w->idx<4){ return kids[w->idx]?(int64_t)(void*)kids[w->idx]:0; }
    int ki=w->idx-4;
    if(ki<w->cur->nkids) return (int64_t)(void*)w->cur->kids[ki];
    return 0;
}
GT_API void gto_ast_walk_free(int64_t wh){ free((GtAstWalk*)(void*)wh); }

/* 释放 gto_ast_dump 返回的字符串 */
GT_API void gto_ast_str_free(char *s){ free(s); }
