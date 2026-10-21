# How Krisp, Zoom, Google, Microsoft and Apple do it: public research (Oct 2026)

**Bottom line.** The *method* is public. The *weights, training data and exact architectures of Krisp, Zoom and Apple are not*. What we can copy is the recipe, and that recipe is mostly **data + training targets + engineering around the model**, not a secret algorithm.

Confidence tags: **[primary]** paper/repo/vendor page read directly, **[secondary]** third-party description, **[inferred]** my reasoning, not stated by anyone.

## 1. What the vendors say (and don't)

| Vendor | What is public | What is not |
|---|---|---|
| **Krisp** | Neural nets trained on "thousands" of noise types, on-device [secondary]. 2019 NVIDIA GTC slides call the model "krispNet" (slides unreadable by our tools). Voice Isolation = one model that does noise cancellation *and* removes non-primary voices [secondary, Pipecat/partner docs]. A partner page says no enrollment is required [secondary]. Public benchmark: 265 real recordings with a second voice, hand-labelled (see §4). Four model sizes: Lite uses ~1/3 of Default's compute for ~1 pt more WER [primary, Krisp blog]. | Architecture, parameter counts, training data, how "primary speaker" is chosen. Krisp states it does not disclose these. No papers (HF org page lists none). |
| **Zoom** | Patent US 11,621,016 [primary]: **cascade** = cheap DSP for stationary noise, then a small CNN/MLP classifier looks at ~10 ms frames (pitch, harmonics, MFCCs, speech features), smooths over ~80 ms, and **only runs the heavy AI denoiser when the signal is still judged noisy**. Settings Auto/Low/Medium/High, higher = more CPU. | The denoiser itself. Patent says nothing about near-field logic. |
| **Google Meet** | 2020 launch: supervised denoiser trained on internal calls + YouTube, from the Limes Audio acquisition, runs **in Google's cloud**, not on the PC [secondary, VentureBeat]. Known limit: speech from people near another participant's mic can pass through. | Anything since 2020. |
| **Microsoft Teams** | Most open of all: DNS Challenge datasets, test sets and a perceptual metric with PCC 0.97 to human ratings [secondary, Cutler talk abstract]. Training used >3,000 real rooms + 115,000 synthetic rooms, laughter/crying kept in the "clean" class [secondary]. Teams "voice isolation" **uses an enrolled voice profile** (user reads a short paragraph) [secondary]. | Production architecture. |
| **Apple** | "Voice Isolation" mic mode uses machine learning [secondary]. 2018 HomePod paper: far-field enhancement in <15% of one 1.4 GHz A8 core [secondary]. | Everything else. **Beamforming / Neural Engine claims on SEO sites are unverified; my earlier mention of multi-mic advantage is plausible but unconfirmed.** |

## 2. The open models that do the same job

| Model | Why it matters | Numbers (self-reported unless noted) | Licence |
|---|---|---|---|
| **DeepFilterNet** 1/2/3 (we use 3) | Same recipe as the commercial ones; trained on DNS4 (multilingual) [primary] | DFN1: RTF 0.19 single-thread on a notebook **i5-8250U** [primary, paper]. DFN2: RTF **0.04** on "a notebook Core-i5" [primary, abstract; exact CPU not named]. | MIT/Apache |
| **PercepNet** (Valin, Amazon) | 32 ERB-band gains + pitch filter instead of dense spectrogram masks: <5% of a core at 48 kHz [primary] | 10 ms frames, 30 ms lookahead | paper only |
| **Personalized PercepNet** | **Keeps only the enrolled voice**: speaker embedding appended to every frame before the GRUs. PPN-512: 8.5 M params, **4.7% of one core on an i7-8565U** (8th gen, same family as your i5) [primary] | Trained with 1 interfering talker + noise, SIR −5…10 dB, SNR −5…35 dB, 4,500+ speakers, 80 h noise. Authors note the target is "typically close to the microphone and the loudest component". PESQ 2.36 vs 1.75 for plain PercepNet. | paper only, no public code found |
| **GTCRN** | 48 k params, 33 MMACs/s, RTF 0.07 on i5-12400 [secondary] | beats RNNoise-class models | MIT |
| **FastEnhancer** (T/B/S/M/L, 16 and 48 kHz) | Speed-optimised design: no layer-norm, fused BN, GRU+attention. M = 492 k params, RTF 0.10 Xeon / 0.17 M1 (ONNX Runtime fp32, one thread) [primary]. Trained on small data (VCTK-DEMAND); 48 kHz checkpoints use broader noise (DNS, WHAM, TUT, DEMAND...) [primary, repo]. | **faster-enhancer.c**: int8 C runtime, **3.3× faster than ONNX fp32** (M2 RTF 0.069 vs 0.230), quality within −0.006 PESQ [primary, paper 2607.25350] | MIT / MIT |
| **voxlock** | Open **target-speaker extraction**: keeps the caller, removes other voices; enrolls from ~3 s of speech (auto-picks loudest 3 s). 27 M-param causal band-split RNN, 16 kHz, 24 ms delay [primary, README] | Claims 4.95% WER and 0.11 leaked words/min on Krisp's benchmark vs Krisp VI 2.5 at 5.30% / 1.52 (**self-reported by a competitor, unverified**). RTF 0.69 on 4 threads of an M2 Pro → **too heavy for an i5-8250U**. Trained in 4 stages on one NVIDIA L4 GPU (~9,200 steps total), starting from the REAL-TSE baseline [primary]. | Apache-2.0 (weights licence not stated) |
| **CoFi-Lite** (Jul 2026) | 83 k params, 12.9 M MACs/s, beats GTCRN | no code found | n/a |

## 3. What these models are doing, precisely

1. **Spectrogram in, filter out.** ~10 ms hop, ~20 ms window. The network outputs gains per frequency band (ERB/Bark-spaced), and in DFN also a short complex filter on the low bins that restores harmonic detail. Same in PercepNet, Teams-style models and FastEnhancer.
2. **Recurrent core.** GRU/LSTM layers carry what the sound has been doing for the past ~second; that is how "steady rain" is told apart from "speech".
3. **Trained on synthetic mixtures.** Clean speech × noise (DNS: 750 h speech, 180 h noise) × simulated and real room impulse responses × random SNR (−5 to 35 dB) × random band-limiting/spectral tilt. Sampling weights let you over-represent hard sounds (the DFN README says to raise the factor for transient noise).
4. **Losses that match hearing.** Multi-resolution spectral loss, log-loudness (ERB), SI-SDR, and in newer work differentiable PESQ and an ASR loss (E3Net). Evaluated with DNSMOS / PESQ / WER.
5. **"Primary speaker" is done one of two ways:** (a) enrollment embedding fed into the net (Personalized PercepNet, Teams, voxlock), or (b) learned from the training data alone, since the target is nearly always the loudest, closest, driest voice (Krisp's partner docs say no enrollment; mechanism undisclosed).
6. **Engineering around the model:** Zoom's cascade, AEC, AGC, VAD gating, quantised runtimes.

## 4. What this changes for our plan

1. **CPU risk is smaller than I feared, and fixable.**
   - DFN2 reported RTF 0.04 on a notebook Core-i5; on our M1 Pro, standard DFN3 measured 0.09 and DFN3-LL 0.17. The paper's i5 is not named, so this is **not proof**, but my "2-3× slower than the M1" guess may be pessimistic for standard DFN3. DFN3-LL is the heavier one and still needs a real measurement.
   - **int8 custom runtime**: faster-enhancer.c showed a 3.3× speed-up at ~no quality loss. PercepNet-style models reach <5% of a core for the same reason (small, hand-optimised, int8-friendly).
   - **Zoom's cascade**: run a cheap detector first, and the heavy model only when needed (rain, barks), idle otherwise.
2. **Unwanted voices: three real options, in order of effort.**
   - *Level/distance gate* (already in the spec): crude but costs nothing.
   - *Enrolled-voice model (Personalized-PercepNet style)*: proven at **4.7% of an 8th-gen i7 core**. Needs us to train it; no public code found. voxlock proves the training recipe is feasible on one rented GPU but its 27 M-param model is too heavy for the i5.
   - *Fine-tune DFN (or FastEnhancer-48k) to ignore far/other voices* using our own mixtures (target = loud near voice; interferer = quieter reverberant voice). This is what Krisp most plausibly does [inferred].
3. **We have a real benchmark for background voices.** Krisp's [VoiceIsolation-Benchmark-Dataset](https://huggingface.co/datasets/Krisp-AI/VoiceIsolation-Benchmark-Dataset): 265 real, hand-labelled headset recordings (Work 96, Call center 106, Phone calls 63), measures WER with a second voice present. **CC BY-NC 4.0**, so personal evaluation only. It replaces my synthetic TTS "yelling". It needs a speech recogniser to score, and it is background *talkers*, not noise.
4. **Don't over-isolate.** Krisp's own benchmark notes aggressive isolation silently deletes parts of the primary voice, and phone-call clips got slightly *worse* (3.48% → 3.91% WER). Our tests must include primary-voice damage, not just leakage.
5. **Expect the gap to Krisp to be data, not code.** None of the sources suggests an architectural secret. The leaders reach quality through scale and curation of training data, which we can partly replace with *targeted* data (the owner's actual rain, dogs, horns, yelling).

## 5. Recommended next steps
1. Measure DFN3 / DFN3-LL on the real i5 (bench pack, in progress).
2. Try FastEnhancer-M/B 48 kHz (MIT) in `qm-bench` on our test set. If its quality on rain/barks/horns is close to DFN3-LL at 1/3 the CPU, it becomes the lighter default. Unknown: its noise coverage (the 48 kHz data list looks broad but the README gives no per-noise evaluation).
3. Prototype Zoom-style cascade and an int8 runtime only if step 1 shows CPU is a problem.
4. Collect the owner's real noise (phone recordings: rain on roof, barking, horns, the yelling man) to replace the synthetic stand-ins; this is also the training data for fine-tuning.
5. Phase 2 decision: level gate first; then fine-tune for "ignore far voices" vs. train a personalised model. Estimate: a few GPU-hours on a rented card, Linux-only DFN trainer.

## Sources
- Zoom patent: <https://patents.google.com/patent/US11621016>
- Krisp benchmark blog: <https://krisp.ai/blog/voice-isolation-benchmark/>, dataset: <https://huggingface.co/datasets/Krisp-AI/VoiceIsolation-Benchmark-Dataset>, org page: <https://krisp-ai-readme.static.hf.space/README.md>
- Google Meet denoiser (2020): <https://venturebeat.com/ai/google-meet-noise-cancellation-ai-cloud-denoiser-g-suite>
- Microsoft Teams / DNS: <https://atscaleconference.com/videos/developing-machine-learning-based-speech-enhancement-models-for-teams-and-skype-ross-cutler>
- PercepNet: <https://arxiv.org/abs/2008.04259>; Personalized PercepNet: <https://arxiv.org/abs/2106.04129>
- DeepFilterNet: <https://arxiv.org/abs/2110.05588>, DFN2 <https://arxiv.org/abs/2205.05474>, DFN3 <https://arxiv.org/abs/2305.08227>, repo <https://github.com/Rikorose/DeepFilterNet>
- FastEnhancer: <https://arxiv.org/abs/2509.21867>, <https://github.com/aask1357/fastenhancer>; faster-enhancer.c: <https://arxiv.org/abs/2607.25350>, <https://github.com/kdrkdrkdr/faster-enhancer.c>
- GTCRN: <https://github.com/xiaobin-rong/gtcrn>; CoFi-Lite: <https://arxiv.org/abs/2607.10142>
- voxlock: <https://github.com/arbazsiddiqui/voxlock>
- E3Net (personalized, distillation): <https://arxiv.org/abs/2204.00771>
- Windows 11 Voice Focus / Voice Clarity (built-in alternative, NPU or software, mixed reports): <https://pureinfotech.com/enable-voice-focus-reduce-background-noise-windows-11/>
