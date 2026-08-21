import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { es } from "../locales/es";
import { en } from "../locales/en";

void i18n.use(initReactI18next).init({
  resources: { es: { translation: es }, en: { translation: en } },
  lng: "es",
  fallbackLng: "es",
  interpolation: { escapeValue: false },
  returnNull: false,
});

export default i18n;
