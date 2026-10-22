# Preset evaluation (qm engine, M1 Pro, 480-sample callbacks)

Model = what calibration picks on this machine. **Noise-only attenuation** (dB, higher = less gets through): 20 s of the noise alone at the level it had in the 0 dB mix, after 10 s of the user's voice so the gate has learned their level. **Audible** = share of 10 ms frames still louder than -60 dBFS. **Voice** = your clean speech through the engine (STOI 1.0 = identical).

## Noise-only attenuation (dB)

| preset | model | delay | rain | rain_steel | dog_bark | car_horn | yelling_far | worst_case |
|---|---|---|---|---|---|---|---|---|
| low-latency | dfn3-ll | 12.5 ms | 19.6 | 13.2 | 7.6 | 47.5 | 0.4 | 14.6 |
| low-latency + guard fe-b | dfn3-ll | 25.7 ms | 30.3 | 19.9 | 9.5 | 51.5 | 3.4 | 19.9 |
| balanced | fe-m | 20.5 ms | 40.6 | 17.9 | 1.0 | 18.4 | 0.2 | 18.6 |
| balanced + guard fe-b | fe-m | 25.7 ms | 42.7 | 21.5 | 5.2 | 19.4 | 3.0 | 23.6 |
| max-noise-removal | dp2 | 52.5 ms | 155.7 | 13.5 | 149.3 | 78.8 | 3.5 | 15.4 |
| max-noise-removal + guard fe-b | dp2 | 55.0 ms | 155.7 | 19.8 | 149.3 | 149.7 | 7.7 | 22.5 |

## Share of frames still audible (> -60 dBFS), %

| preset | rain | rain_steel | dog_bark | car_horn | yelling_far | worst_case |
|---|---|---|---|---|---|---|
| low-latency | 30 | 64 | 10 | 0 | 46 | 69 |
| low-latency + guard fe-b | 2 | 13 | 7 | 0 | 39 | 20 |
| balanced | 2 | 26 | 18 | 7 | 48 | 45 |
| balanced + guard fe-b | 1 | 10 | 13 | 4 | 41 | 16 |
| max-noise-removal | 0 | 53 | 0 | 0 | 43 | 77 |
| max-noise-removal + guard fe-b | 0 | 11 | 0 | 0 | 35 | 18 |

## Mixed speech + noise at 0 dB: SI-SDR gain (dB) / STOI

| preset | rain | rain_steel | dog_bark | car_horn | yelling_far | worst_case |
|---|---|---|---|---|---|---|
| low-latency | +10.1 / 0.67 | +10.1 / 0.88 | +10.4 / 0.98 | +13.7 / 0.97 | +1.7 / 0.84 | +10.5 / 0.77 |
| low-latency + guard fe-b | +9.6 / 0.66 | +8.9 / 0.85 | +7.9 / 0.95 | +7.4 / 0.94 | +1.7 / 0.82 | +9.0 / 0.75 |
| balanced | +9.2 / 0.65 | +10.1 / 0.88 | +11.1 / 0.97 | +6.9 / 0.95 | +6.6 / 0.91 | +9.1 / 0.74 |
| balanced + guard fe-b | +8.9 / 0.64 | +9.0 / 0.86 | +8.4 / 0.95 | +5.1 / 0.93 | +6.4 / 0.90 | +8.3 / 0.74 |
| max-noise-removal | +10.9 / 0.69 | +11.4 / 0.90 | +9.1 / 0.96 | +6.8 / 0.95 | +7.1 / 0.90 | +8.5 / 0.79 |
| max-noise-removal + guard fe-b | +10.4 / 0.68 | +9.9 / 0.88 | +7.8 / 0.95 | +4.9 / 0.92 | +6.6 / 0.88 | +7.8 / 0.78 |

## Your voice with no noise (damage check)

| preset | STOI | SI-SDR dB | level change dB |
|---|---|---|---|
| low-latency | 0.998 | 31.8 | -0.1 |
| low-latency + guard fe-b | 0.992 | 23.4 | -0.2 |
| balanced | 0.999 | 32.0 | -0.0 |
| balanced + guard fe-b | 0.992 | 24.0 | -0.1 |
| max-noise-removal | 0.981 | 20.5 | -0.3 |
| max-noise-removal + guard fe-b | 0.976 | 19.1 | -0.3 |
