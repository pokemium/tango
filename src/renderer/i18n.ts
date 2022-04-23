import i18n from "i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import { initReactI18next } from "react-i18next";

import locales from "./locales";

i18n
  .use({
    type: "backend",
    init: () => {
      void null;
    },
    read: function (
      language: string,
      namespace: string,
      callback: (err: any, data?: { [key: string]: string }) => void
    ) {
      const locale = locales[language];
      if (!locale) {
        callback(`${language} not found`);
        return;
      }
      callback(null, locale.default[namespace]);
    },
  })
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    fallbackLng: "en",

    interpolation: {
      escapeValue: false, // not needed for react as it escapes by default
    },
  });

export default i18n;