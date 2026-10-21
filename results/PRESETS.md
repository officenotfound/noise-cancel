# Preset evaluation (qm engine, M1 Pro, 480-sample callbacks)

Model = what calibration picks on this machine. **Noise-only attenuation** (dB, higher = less gets through): 20 s of the noise alone at the level it had in the 0 dB mix, after 10 s of the user's voice so the gate has learned their level. **Audible** = share of 10 ms frames still louder than -60 dBFS. **Voice** = your clean speech through the engine (STOI 1.0 = identical).

## Noise-only attenuation (dB)

| preset | model | delay | rain | rain_steel | dog_bark | car_horn | yelling_far | worst_case |
|---|---|---|---|---|---|---|---|---|
| low-latency | dfn3-ll | 12.5 ms | 19.6 | 13.2 | 7.6 | 47.5 | 0.4 | 14.6 |
| balanced | fe-m | 20.5 ms | 40.6 | 17.9 | 1.0 | 18.4 | 0.2 | 18.6 |
| max-noise-removal | dp2 | 52.5 ms | 155.7 | 13.5 | 149.3 | 78.8 | 3.5 | 15.4 |
| max-voice-separation | fe-m | 20.5 ms | 40.6 | 17.9 | 1.0 | 18.4 | 0.2 | 18.6 |
| light | fe-b | 23.2 ms | 41.3 | 32.3 | 8.1 | 25.1 | 13.7 | 35.5 |
| low-latency (gate off) | dfn3-ll | 10.0 ms | 19.5 | 13.1 | 5.4 | 33.5 | 0.4 | 14.5 |

## Share of frames still audible (> -60 dBFS), %

| preset | rain | rain_steel | dog_bark | car_horn | yelling_far | worst_case |
|---|---|---|---|---|---|---|
| low-latency | 30 | 64 | 10 | 0 | 46 | 69 |
| balanced | 2 | 26 | 18 | 7 | 48 | 45 |
| max-noise-removal | 0 | 53 | 0 | 0 | 43 | 77 |
| max-voice-separation | 2 | 26 | 18 | 7 | 48 | 45 |
| light | 1 | 3 | 4 | 1 | 12 | 5 |
| low-latency (gate off) | 36 | 64 | 11 | 0 | 46 | 71 |

## Mixed speech + noise at 0 dB: SI-SDR gain (dB) / STOI

| preset | rain | rain_steel | dog_bark | car_horn | yelling_far | worst_case |
|---|---|---|---|---|---|---|
| low-latency | +10.1 / 0.67 | +10.1 / 0.88 | +10.4 / 0.98 | +13.7 / 0.97 | +1.7 / 0.84 | +10.5 / 0.77 |
| balanced | +9.2 / 0.65 | +10.1 / 0.88 | +11.1 / 0.97 | +6.9 / 0.95 | +6.6 / 0.91 | +9.1 / 0.74 |
| max-noise-removal | +10.9 / 0.69 | +11.4 / 0.90 | +9.1 / 0.96 | +6.8 / 0.95 | +7.1 / 0.90 | +8.5 / 0.79 |
| max-voice-separation | +9.2 / 0.65 | +10.1 / 0.88 | +11.1 / 0.97 | +6.9 / 0.95 | +6.6 / 0.91 | +9.1 / 0.74 |
| light | +9.1 / 0.61 | +8.9 / 0.83 | +8.1 / 0.94 | +6.4 / 0.92 | +2.6 / 0.83 | +7.7 / 0.70 |
| low-latency (gate off) | +10.7 / 0.68 | +12.3 / 0.90 | +12.9 / 0.99 | +13.9 / 0.97 | +1.7 / 0.84 | +12.0 / 0.77 |

## Your voice with no noise (damage check)

| preset | STOI | SI-SDR dB | level change dB |
|---|---|---|---|
| low-latency | 0.998 | 31.8 | -0.1 |
| balanced | 0.999 | 32.0 | -0.0 |
| max-noise-removal | 0.981 | 20.5 | -0.3 |
| max-voice-separation | 0.999 | 32.0 | -0.0 |
| light | 0.984 | 26.9 | -0.1 |
| low-latency (gate off) | 0.998 | 31.9 | -0.1 |
