# Q-5 fuzz slices (aarch64 Linux, cargo +nightly fuzz, -max_len=65536 -rss_limit_mb=2048; library = fb35448e in every slice; the commit in the name is the branch head)

```
q5a tok_diff secs=100 execs=35151 rc=0 commit=8e34d0aa [Done 35151 runs in 101 second] cov[cov: 10231 ft: 52114] log=q5a-tok_diff-094430-8e34d0aa.log
q5b tok_diff secs=100 execs=39550 rc=0 commit=8e34d0aa [Done 39550 runs in 101 second] cov[cov: 10290 ft: 52370] log=q5b-tok_diff-094620-8e34d0aa.log
q5c tok_diff secs=100 execs=37970 rc=0 commit=8e34d0aa [Done 37970 runs in 101 second] cov[cov: 10357 ft: 52682] log=q5c-tok_diff-094824-8e34d0aa.log
q5d tok_diff secs=100 execs=41677 rc=0 commit=fe8ca36e [Done 41677 runs in 101 second] cov[cov: 10401 ft: 52971] log=q5d-tok_diff-095028-fe8ca36e.log
q5e tok_diff secs=100 execs=39530 rc=0 commit=fe8ca36e [Done 39530 runs in 101 second] cov[cov: 10444 ft: 53170] log=q5e-tok_diff-095248-fe8ca36e.log
q5f tok_diff secs=100 execs=38634 rc=0 commit=fe8ca36e [Done 38634 runs in 101 second] cov[cov: 10491 ft: 53444] log=q5f-tok_diff-095439-fe8ca36e.log
q5g tok_diff secs=100 execs=45513 rc=0 commit=a375ba81 [Done 45513 runs in 101 second] cov[cov: 10532 ft: 53726] log=q5g-tok_diff-100214-a375ba81.log
q5p1 parse_trace secs=105 execs=143517 rc=0 commit=a375ba81 [Done 143517 runs in 106 second] cov[cov: 17186 ft: 75880] log=q5p1-parse_trace-100409-a375ba81.log
q5p2 parse_trace secs=105 execs=116748 rc=0 commit=a375ba81 [Done 116748 runs in 106 second] cov[cov: 17195 ft: 76027] log=q5p2-parse_trace-100606-a375ba81.log
q5p3 parse_trace secs=100 execs=110115 rc=0 commit=020a0618 [Done 110115 runs in 101 second] cov[cov: 17202 ft: 76097] log=q5p3-parse_trace-101139-020a0618.log
q5p4 parse_trace secs=100 execs=127678 rc=0 commit=020a0618 [Done 127678 runs in 101 second] cov[cov: 17215 ft: 76236] log=q5p4-parse_trace-102910-020a0618.log
q5p5 parse_trace secs=100 execs=112252 rc=0 commit=9ec83562 [Done 112252 runs in 101 second] cov[cov: 17218 ft: 76298] log=q5p5-parse_trace-103240-9ec83562.log
q5p6 parse_trace secs=100 execs=57829 rc=0 commit=9ec83562 [Done 57829 runs in 101 second] cov[cov: 17222 ft: 76326] log=q5p6-parse_trace-103427-9ec83562.log
```

tok_diff: 7 slices, 707 s, 278,025 executions; parse_trace: 6 slices, 616 s, 668,139 executions; no crash, no mismatch.
