import anthropic from "@lobehub/icons-static-svg/icons/anthropic.svg";
import antigravity from "@lobehub/icons-static-svg/icons/antigravity.svg";
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
import type { ProviderId } from "../types";

const LOGO: Record<ProviderId, string> = {
  anthropic,
  openai,
  kimi,
  antigravity,
  zai,
  minimax,
  openrouter,
  xai,
  deepseek,
  gemini,
  groq,
  mistral,
};

/**
 * A provider's mark, drawn in whatever colour it inherits.
 *
 * The logos used to carry their brand hues. On black they now render white,
 * with everything else: colour in this design belongs to the two meter ramps
 * and to nothing else, so a row of eleven brand colours would be eleven claims
 * on the eye competing with the one reading that matters. The marks are already
 * distinct as shapes — that is what a logo is for.
 */
export function ProviderLogo({ provider, size = 18 }: { provider: ProviderId; size?: number }) {
  return (
    <span
      className="provider-logo"
      aria-hidden="true"
      style={
        {
          width: size,
          height: size,
          // Data URLs contain SVG punctuation; quoting keeps the CSS custom
          // property valid so the mask is applied instead of a solid square.
          "--provider-logo": `url("${LOGO[provider]}")`,
        } as CSSProperties
      }
    />
  );
}
