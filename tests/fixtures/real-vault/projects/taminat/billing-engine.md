---
title: تأمينات — Billing Engine
tags: [taminat, billing, settlement]
---

# تأمينات — Billing Engine

محرك الفوترة والتسوية لخدمات السحب والنقل.

## مكوناته

1. **Settlement split**: لكل عملية يُحسب توزيع الإيراد على الأطراف.
2. **Audit hook**: نقطة اتصال لمراجع الشريعة قبل الإقفال النهائي.
3. **Ledger**: سجل ثلاثي القيد، يلتقط كل تغيير.

## القاعدة الجوهرية

```
settlement = (provider_share, platform_share, regulator_share)
invariant : sum(settlement) == 100
```
^settlement-split

نفس البنية الجبرية موجودة في
[[areas/work/sovereign-protocol-overview#^split-rule|بروتوكول Sovereign]].
الـ cortex المفروض يلاحظ التطابق ويقترح استخراج abstraction مشترك.

## API

```rust
pub fn calculate_settlement(amount_halalas: u64, kind: ServiceKind)
    -> Settlement {
    match kind {
        ServiceKind::Tow => Settlement {
            provider_share: 70,
            platform_share: 25,
            regulator_share: 5,
        },
        ServiceKind::Storage => Settlement {
            provider_share: 60,
            platform_share: 35,
            regulator_share: 5,
        },
    }
}
```

## مفتوح

- مراجعة شرعية: انظر [[projects/taminat/shariah-compliance-checklist]].
- الـ regulator API ما زال stub. ينتظر اعتماد المرجعية.
