# Islet Pro

Islet Pro is a one-time purchase that unlocks extra features. Everything Islet did before Pro stays free.

## For users

1. Buy Pro from the **Buy Islet Pro** button in Settings → Islet Pro. Your key arrives by email and on the receipt page.
2. Paste the key in Settings → Islet Pro and click **Activate**.
3. One key works on up to three of your computers. To move it, click **Move Pro to another computer** on the old one first.

**Privacy:**

- Activating sends the key and a label for this computer (for example "Islet on Windows") to the store, so it can check the key.
- After that, Islet asks the store once a week whether the key is still valid, which is how a refunded key stops working.
- Being offline never turns Pro off.
- The key is kept in your system keychain, and nothing else is sent.

From a terminal:

```bash
islet pro activate
```

```bash
islet pro status
```

```bash
islet pro deactivate
```

## For the maintainer: setting up the store

Islet works with **Polar** or **Lemon Squeezy**. Both act as the merchant of record, so they handle payments, sales tax, VAT and refunds, and both issue license keys. You need to be 18 or older to open a seller account. Use whichever you prefer:

| | Polar | Lemon Squeezy |
|---|---|---|
| Sign up | polar.sh | lemonsqueezy.com |
| Product | One-time product, price about $9 | One-time product, price about $9 |
| Keys | Add a **License keys** benefit with an activation limit of 3 | Turn on **Generate license keys** with an activation limit of 3 |
| Ids you need | Organization id (Settings → General) | Store id and product id |

Then, in the GitHub repository, open **Settings → Secrets and variables → Actions → Variables** and add the following. These ids are public (they appear in every checkout link), so they are variables, not secrets:

- **For Polar:** `ISLET_POLAR_ORG`, your organization id.
- **For Lemon Squeezy:** `ISLET_LEMON_STORE` and `ISLET_LEMON_PRODUCT`.
- **For both:** `ISLET_BUY_URL`, the product's checkout link (it must start with `https://`).

The next release is built with these variables, and its Settings shows the Buy button. Builds without them say "Islet Pro isn't on sale in this build".

### Testing

To try Pro features in a development build, start the island with `ISLET_PRO=1`. Release builds ignore this variable.

### About the code

Islet is open source under the MIT License, and that includes the Pro features. Someone could build their own copy with the check removed. Pro is for people who want the ready-made, signed, auto-updating app and want to support it. Keep that in mind before adding anything more aggressive.
