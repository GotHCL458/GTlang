import sys

def fib(n):
    if n < 2:
        return n
    return fib(n-1) + fib(n-2)

def loop_sum():
    s = 0
    for i in range(200000000):
        s += i
    return s

which = sys.argv[1] if len(sys.argv) > 1 else "fib"
if which == "fib":
    print(fib(35))
else:
    print(loop_sum())
