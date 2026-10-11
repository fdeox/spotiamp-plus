#pragma once

// Spotiamp+ addition to projectM (copied in by scripts/build-projectm.mjs):
// get_fft() and get_fft_peak() for presets made for BeatDrop / MilkDrop 3,
// Incubo's takes on Spotiamp+'s own patterns among them. Their shaders read a
// 512x2 texture, row 0 the smoothed spectrum and row 1 its peaks, worked out
// the way BeatDrop does: against the spectrum's overall level (so loud and
// quiet songs look alike), a noise floor taken off, the lowest bins tamed,
// quick to rise and slower to fall (the preset's FFTAttack / FFTDecay), peaks
// held for a moment. It's updated once a frame from projectM's own spectrum
// and kept by the TextureManager, so it goes with the projectM instance and
// its GL context.

#include "Audio/FrameAudioData.hpp"
#include "Renderer/Texture.hpp"
#include "Renderer/TextureManager.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <memory>
#include <regex>
#include <string>

namespace libprojectM {
namespace MilkdropPreset {

class SpotiampFft
{
public:
    static constexpr int Bins = 512;

    //! Put in front of a preset's shader when it calls them (HLSL). Presets
    //! check HAS_FFT_PEAK before using the peaks, as BeatDrop defines it.
    static constexpr const char* Helpers =
        "#define HAS_FFT_PEAK 1\n"
        "float get_fft(float pos) { return sqrt(tex2D(sampler_fft, float2(saturate(pos), 0.25)).x); }\n"
        "float get_fft_hz(float freq) { return get_fft(freq / 22050.0); }\n"
        "float get_fft_peak(float pos) { return sqrt(tex2D(sampler_fft, float2(saturate(pos), 0.75)).x); }\n"
        "float get_fft_peak_hz(float freq) { return get_fft_peak(freq / 22050.0); }\n";

    //! A preset's shader, ready for projectM: the helpers in front when it
    //! calls them, and its HAS_FFT_PEAK checks settled, since projectM's
    //! preprocessor only takes "#if <number>" (no defined(), no #ifdef).
    static auto Prepare(const std::string& code) -> std::string
    {
        if (code.find("get_fft") == std::string::npos && code.find("HAS_FFT_PEAK") == std::string::npos)
        {
            return code;
        }
        std::string out = std::regex_replace(code, std::regex(R"(#[ \t]*if[ \t]*!+[ \t]*defined[ \t]*\(?[ \t]*HAS_FFT_PEAK[ \t]*\)?)"), "#if 0");
        out = std::regex_replace(out, std::regex(R"(#[ \t]*if[ \t]+defined[ \t]*\(?[ \t]*HAS_FFT_PEAK[ \t]*\)?)"), "#if 1");
        out = std::regex_replace(out, std::regex(R"(#[ \t]*ifndef[ \t]+HAS_FFT_PEAK)"), "#if 0");
        out = std::regex_replace(out, std::regex(R"(#[ \t]*ifdef[ \t]+HAS_FFT_PEAK)"), "#if 1");
        return Helpers + out;
    }

    static auto Get() -> SpotiampFft&
    {
        static SpotiampFft instance;
        return instance;
    }

    //! The texture, made on first use for this projectM instance.
    auto FftTexture(Renderer::TextureManager& textures) -> std::shared_ptr<Renderer::Texture>
    {
        if (!textures.spotiampFft)
        {
            textures.spotiampFft = std::make_shared<Renderer::Texture>("fft", Bins, 2, GL_R32F, GL_RED, GL_FLOAT, false);
            m_frame = -1; // a new instance counts its frames from 0 again
        }
        return textures.spotiampFft;
    }

    //! Once a frame (a second call in the same frame, as in a transition, does nothing).
    void Update(Renderer::TextureManager& textures, const Audio::FrameAudioData& audio,
                int frame, float fps, float attack, float decay)
    {
        if (!textures.spotiampFft || frame == m_frame)
        {
            return;
        }
        m_frame = frame;
        if (!(fps > 1.0f))
        {
            fps = 60.0f;
        }

        // projectM's spectrum is equalised: bin i times -0.02 * ln((512 - i) / 512).
        // BeatDrop reads the plain one, so that comes off again (bin 0 is
        // nothing but the DC offset: left at 0).
        std::array<float, Bins> mono{};
        float sumSquares = 0.0f;
        for (int i = 1; i < Bins; i++)
        {
            float const equalised = -0.02f * std::log(static_cast<float>(Bins - i) / static_cast<float>(Bins));
            mono[i] = 0.5f * (audio.spectrumLeft[i] + audio.spectrumRight[i]) / equalised;
            sumSquares += mono[i] * mono[i];
        }

        // the overall level, quick to follow a loud passage, slow to let go
        float const rms = std::max(std::sqrt(sumSquares / Bins), 1e-5f);
        float const follow = std::exp(-1.0f / fps / (rms > m_level ? 2.0f : 5.0f));
        m_level = m_level * follow + rms * (1.0f - follow);

        auto const unit = [](float v) { return std::min(std::max(v, 0.0f), 1.0f); };
        float const decayFactor = (1.0f - unit(decay)) * (1.0f - unit(decay));
        float const rise = 1.0f - std::pow(1.0f - unit(attack), 60.0f / fps);
        float const fall = 1.0f - std::pow(1.0f - decayFactor, 60.0f / fps);
        for (int i = 0; i < Bins; i++)
        {
            float value = std::max(mono[i] / m_level * Scaling - NoiseFloor, 0.0f);
            if (i < BassBins)
            {
                float const t = static_cast<float>(i) / BassBins;
                value *= 0.15f + 0.85f * t * t;
            }
            float& smooth = m_rows[i];
            smooth += (value - smooth) * (value > smooth ? rise : fall);
            if (smooth < 5e-8f)
            {
                smooth = 0.0f;
            }
            float& peak = m_rows[Bins + i];
            if (smooth >= peak)
            {
                peak = smooth;
                m_hold[i] = fps * (0.25f + decay * 0.75f);
            }
            else if (m_hold[i] > 0.0f)
            {
                m_hold[i] -= 1.0f;
            }
            else
            {
                peak *= std::exp(-1.0f / fps / 0.3f);
            }
        }

        // upload, leaving whatever was bound bound again
        GLint bound = 0;
        glGetIntegerv(GL_TEXTURE_BINDING_2D, &bound);
        glBindTexture(GL_TEXTURE_2D, textures.spotiampFft->TextureID());
        glTexSubImage2D(GL_TEXTURE_2D, 0, 0, 0, Bins, 2, GL_RED, GL_FLOAT, m_rows.data());
        glBindTexture(GL_TEXTURE_2D, static_cast<GLuint>(bound));
    }

private:
    static constexpr float Scaling = 0.175f;  //!< BeatDrop's default
    static constexpr float NoiseFloor = 0.03f; //!< BeatDrop's default
    static constexpr int BassBins = 12;        //!< up to ~500 Hz, BeatDrop's 24 of 1024

    std::array<float, 2 * Bins> m_rows{}; //!< the texture's two rows
    std::array<float, Bins> m_hold{};     //!< frames each peak still holds
    float m_level{0.1f};
    int m_frame{-1};
};

} // namespace MilkdropPreset
} // namespace libprojectM
