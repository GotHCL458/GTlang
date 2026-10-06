#include <stdio.h>
#include <stdint.h>
extern int64_t gto_ast_lit_int(int64_t);
extern int64_t gto_ast_binary(int, int64_t, int64_t);
extern char* gto_ast_dump(int64_t);
int main(){
  int64_t a = gto_ast_lit_int(3);
  int64_t b = gto_ast_lit_int(4);
  printf("a ok\n");
  int64_t c = gto_ast_binary(43, a, b);
  printf("c ok\n");
  printf("%s\n", gto_ast_dump(c));
  return 0;
}
