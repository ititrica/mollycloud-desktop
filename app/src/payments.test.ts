import { describe, expect, it } from 'vitest';
import { amountError, checkoutSchema, createdOrderSchema, quote, visibleMethods, planVisible, planPurchasable } from './payments';
const checkout=()=>checkoutSchema.parse({payment_enabled:true,methods:{alipay:{available:true,currency:'CNY',single_min:1,single_max:500},stripe:{available:true,currency:'USD'},yen:{available:true,currency:'JPY'}},plans:[],balance_recharge_multiplier:0.14,subscription_usd_to_cny_rate:7,recharge_fee_rate:2,recharge_bonus_tiers:[{min_amount:100,bonus_percent:20}]});
describe('payment amounts and server contracts',()=>{
  it('separates CNY payment, fee and USD balance bonus',()=>{
    expect(quote(checkout(),100,'alipay')).toEqual({currency:'CNY',base:100,credit:16.8,bonus:2.8,fee:2,total:102});
  });
  it('discount affects payment while preserving credited amount',()=>{
    const info=checkout();info.recharge_bonus_mode='discount';
    expect(quote(info,100,'alipay')).toMatchObject({base:80,credit:14,bonus:0,fee:1.6,total:81.6});
    info.recharge_bonus_tiers=[{min_amount:1,bonus_percent:100}];
    expect(quote(info,100,'alipay').base).toBe(100);
  });
  it('converts subscription amounts only for CNY and rounds fees upward',()=>{
    const info=checkout(),plan={id:1,name:'Pro',description:'',price:20,validity_days:30,validity_unit:'day',features:[],for_sale:true,user_visible:true};
    expect(quote(info,20,'alipay',plan)).toMatchObject({base:140,fee:2.8,total:142.8,credit:0});
    expect(quote(info,20,'stripe',plan)).toMatchObject({base:20,fee:0.4,total:20.4,currency:'USD'});
    expect(quote(info,20,'yen',plan)).toMatchObject({base:20,fee:1,total:21});
  });
  it('checks discounted base against channel limits and daily remaining',()=>{
    const info=checkout();info.recharge_bonus_mode='discount';
    expect(amountError(info,600,'alipay')).toBe('');
    expect(amountError(info,700,'alipay')).toContain('最高支付');
    info.methods.alipay!.daily_limit=1000;info.methods.alipay!.daily_remaining=50;
    expect(amountError(info,100,'alipay')).toContain('剩余额度不足');
  });
  it('does not reject current upstream methods without a daily remaining field',()=>{
    const info=checkout();info.methods.alipay!.daily_limit=1000;
    expect(amountError(info,100,'alipay')).toBe('');
    const parsed=checkoutSchema.parse({...info,plans:[{id:1,name:'Pro',price:10,validity_days:30,for_sale:true,features:'First feature\nSecond feature'}]});
    expect(parsed.plans[0]!.features).toEqual(['First feature','Second feature']);
  });
  it('uses the deployed catalog flags: listed closed plans remain visible, unlisted plans stay hidden',()=>{
    const info=checkoutSchema.parse({...checkout(),plans:[
      {id:1,name:'Display only',price:20,validity_days:30,for_sale:true,user_visible:false},
      {id:2,name:'Unlisted',price:20,validity_days:30,for_sale:false,user_visible:true},
      {id:3,name:'Legacy',price:20,validity_days:30,for_sale:true},
      {id:4,name:'Unlisted closed',price:20,validity_days:30,for_sale:false,user_visible:false},
    ]});
    expect(info.plans.filter(planVisible).map(p=>p.id)).toEqual([1,3]);
    expect(info.plans.filter(planPurchasable).map(p=>p.id)).toEqual([3]);
    expect(amountError(info,20,'alipay',info.plans[0])).toContain('暂不可购买');
    expect(amountError(info,20,'alipay',info.plans[1])).toContain('暂不可购买');
  });
  it('refreshing purchase eligibility does not remove listed plans from the catalog',()=>{
    const info=checkoutSchema.parse({...checkout(),plans:[{id:1,name:'Pro',price:20,validity_days:30,for_sale:true,user_visible:true}]});
    const plan=info.plans[0]!;
    expect(planPurchasable(plan)).toBe(true);
    plan.user_visible=false;
    expect(planVisible(plan)).toBe(true);
    expect(amountError(info,20,'alipay',plan)).toContain('暂不可购买');
    plan.user_visible=true;
    expect(amountError(info,20,'alipay',plan)).toBe('');
    plan.for_sale=false;
    expect(planVisible(plan)).toBe(false);
    expect(planPurchasable(plan)).toBe(false);
  });
  it('canonical method wins regardless of alias ordering',()=>{
    const info=checkout();info.methods.alipay_direct={...info.methods.alipay!,available:false};
    expect(visibleMethods(info).alipay!.available).toBe(true);
  });
  it('parses legacy JSON features and never keeps gateway secrets in the UI',()=>{
    const result=createdOrderSchema.parse({order_id:1,amount:1,pay_amount:7,expires_at:'2026-10-01',can_open:true,client_secret:'private',pay_url:'https://gateway.example'});
    expect(result).not.toHaveProperty('client_secret');expect(result).not.toHaveProperty('pay_url');
    const info=checkoutSchema.parse({...checkout(),plans:[{id:1,name:'Pro',price:1,validity_days:30,for_sale:true,features:'["Daily quota"]'}]});
    expect(info.plans[0]!.features).toEqual(['Daily quota']);
  });
});
