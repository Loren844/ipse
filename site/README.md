# Ipse website

The public site, a single static page: `index.html` with its screenshots in `img/`, in English and French, light and dark. No build step.

- **Published** by `.github/workflows/pages.yml` on every push to `main` that touches `site/`.
- **Visits** are counted by [GoatCounter](https://www.goatcounter.com), without cookies; the script tag sits in the page's `<head>`.
- **Sharing image** `og.png` (1200 × 630). When the site moves to its own domain, update `og:image` in the `<head>` and add a `CNAME` file here.

To preview, open `index.html` in a browser.
