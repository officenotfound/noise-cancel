# Phase 0 results

Machine: Apple M1 Pro (arm64), qm-bench release build. attenuation limit: default 100 dB. Averages are over SNR -5/0/5/10 dB. SI-SDR gain and STOI are against the clean reference; *pause suppression* = how many dB quieter the output is than the input in frames where the target speaker is silent (higher = noise/others removed between words).

## SI-SDR gain (dB, higher is better)

| category | rnnoise | dfn3 | dfn3-ll | fe-t | fe-b | fe-s | fe-m | fe-l |
|---|---|---|---|---|---|---|---|---|
| rain | +5.6 | +9.2 | +9.5 | +7.7 | +8.7 | +9.2 | +9.1 | +9.7 |
| rain_steel *(synthetic noise)* | +6.7 | +10.1 | +11.1 | +7.0 | +9.4 | +10.0 | +11.3 | +11.5 |
| dog_bark | -0.2 | +10.5 | +11.4 | +5.9 | +9.5 | +9.4 | +12.6 | +13.3 |
| car_horn | +5.7 | +11.4 | +12.7 | +6.6 | +9.8 | +11.0 | +11.5 | +11.7 |
| yelling_far *(synthetic noise)* | +0.2 | +1.8 | +1.4 | +1.9 | +3.0 | +4.5 | +6.2 | +7.4 |
| babble | -0.8 | -7.6 | -1.1 | -2.8 | +0.1 | -1.8 | -2.0 | +0.8 |
| worst_case *(synthetic noise)* | +3.1 | +7.3 | +10.8 | +5.9 | +6.6 | +7.1 | +9.0 | +8.9 |

## STOI after processing (0-1, higher is better; input shown first)

| category |  input |rnnoise | dfn3 | dfn3-ll | fe-t | fe-b | fe-s | fe-m | fe-l |
|---|---|---|---|---|---|---|---|---|---|
| rain | 0.61 | 0.65 | 0.70 | 0.72 | 0.65 | 0.66 | 0.68 | 0.71 | 0.71 |
| rain_steel *(synthetic noise)* | 0.79 | 0.84 | 0.89 | 0.91 | 0.77 | 0.85 | 0.88 | 0.91 | 0.91 |
| dog_bark | 0.91 | 0.92 | 0.98 | 0.99 | 0.96 | 0.96 | 0.96 | 0.98 | 0.97 |
| car_horn | 0.90 | 0.92 | 0.96 | 0.97 | 0.93 | 0.95 | 0.95 | 0.97 | 0.98 |
| yelling_far *(synthetic noise)* | 0.85 | 0.84 | 0.85 | 0.87 | 0.85 | 0.87 | 0.89 | 0.91 | 0.93 |
| babble | 0.68 | 0.65 | 0.50 | 0.69 | 0.57 | 0.61 | 0.60 | 0.60 | 0.66 |
| worst_case *(synthetic noise)* | 0.64 | 0.66 | 0.76 | 0.80 | 0.67 | 0.72 | 0.73 | 0.78 | 0.77 |

## Pause suppression (dB, higher is better; capped at 60 = output is digital silence)

| category | rnnoise | dfn3 | dfn3-ll | fe-t | fe-b | fe-s | fe-m | fe-l |
|---|---|---|---|---|---|---|---|---|
| rain | 46.8 | 45.7 | 41.1 | 43.6 | 60.0 | 60.0 | 60.0 | 60.0 |
| rain_steel *(synthetic noise)* | 15.3 | 48.1 | 38.1 | 42.9 | 45.6 | 56.3 | 45.5 | 44.1 |
| dog_bark | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| car_horn | 30.0 | 35.0 | 29.8 | 33.8 | 30.6 | 35.3 | 31.7 | 34.5 |
| yelling_far *(synthetic noise)* | 1.1 | 10.2 | 7.5 | 9.8 | 13.3 | 12.4 | 9.5 | 13.5 |
| babble | 0.7 | 5.9 | 13.9 | 3.5 | 10.6 | 6.5 | 14.9 | 12.2 |
| worst_case *(synthetic noise)* | 38.7 | 46.0 | 41.9 | 60.0 | 60.0 | 60.0 | 60.0 | 60.0 |

## Speed and delay (this machine; NOT the target i5-8250U)

| model | hop | algorithmic delay | measured delay | frame mean | frame p99 | frame max | RTF (= share of one core) | frames over hop |
|---|---|---|---|---|---|---|---|---|
| rnnoise | 10.0 ms | 10.0 ms | 10.0 ms | 0.04 ms | 0.05 ms | 0.15 ms | 0.4% | 0 |
| dfn3 | 10.0 ms | 30.0 ms | 30.0 ms | 0.90 ms | 1.02 ms | 8.20 ms | 9.0% | 0 |
| dfn3-ll | 10.0 ms | 10.0 ms | 10.0 ms | 1.70 ms | 1.86 ms | 11.92 ms | 17.0% | 1 |
| fe-t | 10.7 ms | 10.7 ms | 10.7 ms | 0.16 ms | 0.19 ms | 0.40 ms | 1.5% | 0 |
| fe-b | 10.7 ms | 10.7 ms | 10.7 ms | 0.39 ms | 0.44 ms | 1.12 ms | 3.7% | 0 |
| fe-s | 10.7 ms | 10.7 ms | 10.7 ms | 0.56 ms | 0.63 ms | 1.89 ms | 5.3% | 0 |
| fe-m | 6.7 ms | 14.7 ms | 14.7 ms | 1.28 ms | 1.44 ms | 7.79 ms | 19.2% | 2 |
| fe-l | 4.2 ms | 17.2 ms | 17.2 ms | 2.88 ms | 3.14 ms | 30.26 ms | 69.2% | 74 |
