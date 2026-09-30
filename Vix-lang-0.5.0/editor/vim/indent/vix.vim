if exists("b:did_indent")
  finish
endif
let b:did_indent = 1

setlocal autoindent
setlocal indentexpr=VixIndent(v:lnum)
setlocal indentkeys=0{,0},0),0],:,!^F,o,O

if exists("*VixIndent")
  finish
endif

function! VixIndent(lnum) abort
  let l:prev = prevnonblank(a:lnum - 1)
  if l:prev == 0
    return 0
  endif

  let l:ind = indent(l:prev)
  let l:prevline = getline(l:prev)
  let l:line = getline(a:lnum)

  if l:prevline =~ '{\s*\%(//.*\)\?$'
    let l:ind += shiftwidth()
  endif

  if l:line =~ '^\s*[})\]]'
    let l:ind -= shiftwidth()
  endif

  return max([l:ind, 0])
endfunction
