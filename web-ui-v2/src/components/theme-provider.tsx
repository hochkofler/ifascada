import type { ComponentProps, JSX } from "react";
import { ThemeProvider as NextThemesProvider, useTheme } from "next-themes";

/**
 * App-wide theme provider (next-themes) with ifahub defaults: class-based dark
 * mode (matches the `.dark` selector in globals.css), system preference enabled,
 * and no transition flash on theme change. Apps can override via props.
 */
export function ThemeProvider(props: ComponentProps<typeof NextThemesProvider>): JSX.Element {
  return (
    <NextThemesProvider
      attribute="class"
      defaultTheme="system"
      enableSystem
      disableTransitionOnChange
      {...props}
    />
  );
}

export { useTheme };
