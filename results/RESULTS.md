# Phase 0 results

Machine: Apple M1 Pro (arm64), qm-bench release build. attenuation limit: default 100 dB. Averages are over SNR -5/0/5/10 dB. SI-SDR gain and STOI are against the clean reference; *pause suppression* = how many dB quieter the output is than the input in frames where the target speaker is silent (higher = noise/others removed between words).

## SI-SDR gain (dB, higher is better)

| category | rnnoise | dfn3 | dfn3-ll |
|---|---|---|---|
| rain | +5.6 | +9.2 | +9.5 |
| rain_steel *(synthetic noise)* | +6.7 | +10.1 | +11.1 |
| dog_bark | -0.2 | +10.5 | +11.4 |
| car_horn | +5.7 | +11.4 | +12.7 |
| yelling_far *(synthetic noise)* | +0.2 | +1.8 | +1.4 |
| babble | -0.8 | -7.6 | -1.1 |
| worst_case *(synthetic noise)* | +3.1 | +7.3 | +10.8 |

## STOI after processing (0-1, higher is better; input shown first)

| category |  input |rnnoise | dfn3 | dfn3-ll |
|---|---|---|---|---|
| rain | 0.61 | 0.65 | 0.70 | 0.72 |
| rain_steel *(synthetic noise)* | 0.79 | 0.84 | 0.89 | 0.91 |
| dog_bark | 0.91 | 0.92 | 0.98 | 0.99 |
| car_horn | 0.90 | 0.92 | 0.96 | 0.97 |
| yelling_far *(synthetic noise)* | 0.85 | 0.84 | 0.85 | 0.87 |
| babble | 0.68 | 0.65 | 0.50 | 0.69 |
| worst_case *(synthetic noise)* | 0.64 | 0.66 | 0.76 | 0.80 |

## Pause suppression (dB, higher is better)

| category | rnnoise | dfn3 | dfn3-ll |
|---|---|---|---|
| rain | 46.8 | 45.7 | 41.1 |
| rain_steel *(synthetic noise)* | 15.3 | 48.1 | 38.1 |
| dog_bark | n/a | n/a | n/a |
| car_horn | 30.0 | 35.0 | 29.8 |
| yelling_far *(synthetic noise)* | 1.1 | 10.2 | 7.5 |
| babble | 0.7 | 5.9 | 13.9 |
| worst_case *(synthetic noise)* | 38.7 | 46.0 | 41.9 |

## Speed and delay (this machine; NOT the target i5-8250U)

| model | algorithmic delay | measured delay | frame mean | frame p99 | frame max | RTF (= share of one core) | frames over 10 ms |
|---|---|---|---|---|---|---|---|
| rnnoise | 10 ms | 10 ms | 0.04 ms | 0.05 ms | 0.17 ms | 0.4% | 0 |
| dfn3 | 30 ms | 30 ms | 0.91 ms | 1.06 ms | 2.67 ms | 9.1% | 0 |
| dfn3-ll | 10 ms | 10 ms | 1.71 ms | 1.83 ms | 5.65 ms | 17.1% | 0 |
