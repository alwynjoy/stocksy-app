/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        app: {
          bg: "#F8FAFC",
          surface: "#FFFFFF",
          border: "#E2E8F0",
          text: "#0F172A",
          muted: "#64748B",
          sidebar: "#F1F5F9",
          positive: "#10B981",
          warning: "#F59E0B",
          negative: "#EF4444",
        },
      },
    },
  },
  plugins: [],
};