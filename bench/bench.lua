local function fib(n)
    if n < 2 then return n end
    return fib(n-1) + fib(n-2)
end

local function loop_sum()
    local s = 0
    for i = 0, 200000000-1 do
        s = s + i
    end
    return s
end

local which = arg[1] or "fib"
if which == "fib" then
    print(fib(35))
else
    print(loop_sum())
end
