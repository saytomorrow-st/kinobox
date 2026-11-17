import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bg: "#000000",
        surface: "rgba(255, 255, 255, 0.04)",
        text: "#F5F5F7",
        "text-2": "#86868B",
        accent: "#FA233B",
        premium: "#FFD60A",
        status: {
          open: "#30D158",
          login: "#FFD60A",
          challenge: "#FF9F0A",
          dead: "#FF453A",
          unknown: "#86868B",
        },
      },
      fontFamily: {
        sans: ['"SF Pro Display"', "-apple-system", "BlinkMacSystemFont", "sans-serif"],
      },
      borderRadius: {
        card: "12px",
        button: "8px",
        modal: "20px",
      },
      fontSize: {
        hero: ["42px", { lineHeight: "1.1", fontWeight: "700" }],
        section: ["24px", { lineHeight: "1.2", fontWeight: "600" }],
        card: ["15px", { lineHeight: "1.4" }],
      },
      spacing: {
        page: "60px",
        section: "40px",
      },
    },
  },
  plugins: [],
} satisfies Config;
