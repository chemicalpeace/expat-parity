# Part 2 fuzz slices (aarch64 Linux, cargo +nightly fuzz, -max_len=65536 -rss_limit_mb=2048; library = 26896e38 storeAtts fix in every slice; the commit in the name is the branch head: 4d5b8e2b scaling test, 7b0ff6e9 docs only)

Working corpora: committed fuzz/corpus/<target> (parse_trace 13,869, buffer_diff 10,766) plus, for parse_trace only, 40 seeds ns-dup-* (10 start tags with prefixed attributes, two prefixes bound to one URI, x 4 selector pairs incl. triplets). Corpus not merged back into fuzz/corpus.

```
p2a parse_trace secs=95 execs=102557 rc=0 commit=4d5b8e2b [Done 102557 runs in 98 second] cov[cov: 17290 ft: 76266] log=p2a-parse_trace-143756-4d5b8e2b.log
p2b parse_trace secs=95 execs=119791 rc=0 commit=4d5b8e2b [Done 119791 runs in 96 second] cov[cov: 17305 ft: 76358] log=p2b-parse_trace-143952-4d5b8e2b.log
p2c parse_trace secs=95 execs=117470 rc=0 commit=4d5b8e2b [Done 117470 runs in 96 second] cov[cov: 17316 ft: 76433] log=p2c-parse_trace-144200-4d5b8e2b.log
p2d parse_trace secs=95 execs=111628 rc=0 commit=4d5b8e2b [Done 111628 runs in 99 second] cov[cov: 17322 ft: 76584] log=p2d-parse_trace-144355-4d5b8e2b.log
p2e parse_trace secs=95 execs=99051 rc=0 commit=4d5b8e2b [Done 99051 runs in 96 second] cov[cov: 17329 ft: 76627] log=p2e-parse_trace-144546-4d5b8e2b.log
p2f parse_trace secs=95 execs=114215 rc=0 commit=4d5b8e2b [Done 114215 runs in 96 second] cov[cov: 17338 ft: 76690] log=p2f-parse_trace-144735-4d5b8e2b.log
p2g parse_trace secs=95 execs=118783 rc=0 commit=4d5b8e2b [Done 118783 runs in 96 second] cov[cov: 17352 ft: 76810] log=p2g-parse_trace-144924-4d5b8e2b.log
b2a buffer_diff secs=95 execs=602135 rc=0 commit=7b0ff6e9 [Done 602135 runs in 96 second] cov[cov: 12893 ft: 50211] log=b2a-buffer_diff-145151-7b0ff6e9.log
b2b buffer_diff secs=95 execs=631746 rc=0 commit=7b0ff6e9 [Done 631746 runs in 96 second] cov[cov: 12896 ft: 50368] log=b2b-buffer_diff-145341-7b0ff6e9.log
b2c buffer_diff secs=95 execs=608069 rc=0 commit=7b0ff6e9 [Done 608069 runs in 96 second] cov[cov: 12908 ft: 50540] log=b2c-buffer_diff-145529-7b0ff6e9.log
b2d buffer_diff secs=95 execs=645314 rc=0 commit=7b0ff6e9 [Done 645314 runs in 96 second] cov[cov: 12911 ft: 50657] log=b2d-buffer_diff-145718-7b0ff6e9.log
b2e buffer_diff secs=95 execs=588489 rc=0 commit=7b0ff6e9 [Done 588489 runs in 96 second] cov[cov: 12915 ft: 50934] log=b2e-buffer_diff-145906-7b0ff6e9.log
b2f buffer_diff secs=95 execs=619606 rc=0 commit=7b0ff6e9 [Done 619606 runs in 96 second] cov[cov: 12916 ft: 51049] log=b2f-buffer_diff-150056-7b0ff6e9.log
b2g buffer_diff secs=95 execs=600993 rc=0 commit=7b0ff6e9 [Done 600993 runs in 96 second] cov[cov: 12917 ft: 51209] log=b2g-buffer_diff-150251-7b0ff6e9.log
```

Totals: parse_trace 7 slices, 677 s, 783,495 executions; buffer_diff 7 slices, 672 s, 4,296,352 executions; rc=0 in every slice, no crash, no mismatch.
