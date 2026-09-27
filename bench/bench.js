function fib(n) {
    if (n < 2) return n;
    return fib(n-1) + fib(n-2);
}
function loop_sum() {
    let s = 0;
    for (let i = 0; i < 200000000; i++) s += i;
    return s;
}
const which = process.argv[2] || 'fib';
if (which === 'fib') console.log(fib(35));
else console.log(loop_sum());
