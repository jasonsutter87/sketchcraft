// Help > About SketchCraft: the name, the promise, the links, and a thank-you to the people who wrote the code.
// The thank-you list is not kept by hand: it is read from git history when the app is built (app/contributors.mjs).
// the app icon itself (256 px), so About shows exactly what is in the Dock / taskbar
import appIcon from "../src-tauri/icons/128x128@2x.png";
import { openLink, LINKS, type LinkName } from "./links";
import { useLocale } from "./locale";
import { Sheet } from "./sheet";

const WHERE: [LinkName, string][] = [
  ["repo", "about.github"],
  ["license", "about.license"],
  ["changelog", "about.changelog"],
];

export function AboutDialog({ onClose }: { onClose: () => void }) {
  const { t } = useLocale();
  return (
    <Sheet onClose={onClose} label={t("menu.about")} className="about" width={460}>
      <div className="about-head">
        <img className="about-logo" src={appIcon} alt="" width={96} height={96} />
        <h2 className="about-name">SketchCraft</h2>
        <p className="about-version">
          {t("about.version")} {__APP_VERSION__}
        </p>
        <p className="about-tagline">{t("about.tagline")}</p>
        <p className="about-promise">{t("about.promise")}</p>
      </div>
      <ul className="about-links">
        {WHERE.map(([name, label]) => (
          <li key={name}>
            <a
              href={LINKS[name]}
              target="_blank"
              rel="noopener noreferrer"
              onClick={(e) => {
                e.preventDefault();
                openLink(name);
              }}
            >
              {t(label)}
            </a>
          </li>
        ))}
      </ul>
      {__CONTRIBUTORS__.length > 0 && (
        <section className="about-thanks" aria-labelledby="about-thanks-title">
          <h3 id="about-thanks-title">{t("about.thanks")}</h3>
          <p>{t("about.thanksNote")}</p>
          <ul>
            {__CONTRIBUTORS__.map((c) => (
              <li key={c.login ?? c.name}>{c.name}</li>
            ))}
          </ul>
        </section>
      )}
      <div className="about-foot">
        <button className="btn small primary" onClick={onClose}>
          {t("about.close")}
        </button>
      </div>
    </Sheet>
  );
}
