import anthropic from "@lobehub/icons-static-svg/icons/anthropic.svg";
import deepseek from "@lobehub/icons-static-svg/icons/deepseek.svg";
import gemini from "@lobehub/icons-static-svg/icons/gemini.svg";
import groq from "@lobehub/icons-static-svg/icons/groq.svg";
import mistral from "@lobehub/icons-static-svg/icons/mistral.svg";
import openai from "@lobehub/icons-static-svg/icons/openai.svg";
import openrouter from "@lobehub/icons-static-svg/icons/openrouter.svg";
import xai from "@lobehub/icons-static-svg/icons/xai.svg";
import kimi from "@lobehub/icons-static-svg/icons/kimi.svg";
import minimax from "@lobehub/icons-static-svg/icons/minimax.svg";
import zai from "@lobehub/icons-static-svg/icons/zai.svg";
import type { CSSProperties } from "react";
import { ACCENT, type ProviderId } from "../types";

const LOGO: Record<ProviderId, string> = {
  anthropic,
  openai,
  kimi,
  zai,
  minimax,
  openrouter,
  xai,
  deepseek,
  gemini,
  groq,
  mistral,
};

export function ProviderLogo({ provider, size = 18 }: { provider: ProviderId; size?: number }) {
  return (
    <span
      className="provider-logo"
      aria-hidden="true"
      style={
        {
          width: size,
          height: size,
          color: ACCENT[provider],
          // Data URLs contain SVG punctuation; quoting keeps the CSS custom
          // property valid so the mask is applied instead of a solid square.
          "--provider-logo": `url("${LOGO[provider]}")`,
        } as CSSProperties
      }
    />
  );
}
