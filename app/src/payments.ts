import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';
const amount = z.number().finite().nonnegative();
const methodSchema=z.object({currency:z.string().optional(),display_name:z.string().optional(),daily_limit:amount.default(0),daily_remaining:amount.optional(),single_min:amount.default(0),single_max:amount.default(0),fee_rate:amount.default(0),available:z.boolean()});
const features=z.preprocess(value=>{if(typeof value==='string'){try{return JSON.parse(value);}catch{return value.split(/\r?\n/).map(s=>s.trim()).filter(Boolean);}}return value??[];},z.array(z.string()));
const planSchema=z.object({id:z.number().int().positive(),name:z.string(),description:z.string().default(''),price:amount,original_price:amount.nullable().optional(),currency:z.string().optional(),validity_days:amount,validity_unit:z.string().default('day'),features,for_sale:z.boolean(),user_visible:z.boolean().default(true),group_platform:z.string().optional(),group_name:z.string().optional(),rate_multiplier:amount.optional(),daily_limit_usd:amount.nullable().optional(),weekly_limit_usd:amount.nullable().optional(),monthly_limit_usd:amount.nullable().optional()});
export const checkoutSchema=z.object({payment_enabled:z.boolean(),methods:z.record(z.string(),methodSchema),global_min:amount.default(0),global_max:amount.default(0),plans:z.array(planSchema),balance_disabled:z.boolean().default(false),balance_recharge_multiplier:amount.default(1),subscription_usd_to_cny_rate:amount.default(0),recharge_fee_rate:amount.optional(),recharge_bonus_tiers:z.array(z.object({min_amount:amount,bonus_percent:amount})).nullish(),recharge_bonus_mode:z.string().default('bonus'),recharge_bonus_notice:z.string().default(''),help_text:z.string().default('')});
export const orderSchema=z.object({id:z.number().int().positive(),amount,pay_amount:amount,currency:z.string().optional(),fee_rate:amount.default(0),bonus_amount:amount.optional(),payment_type:z.string(),out_trade_no:z.string(),status:z.string(),order_type:z.string(),created_at:z.string(),expires_at:z.string(),plan_id:z.number().nullish(),refund_amount:amount.default(0)});
const ordersSchema=z.object({items:z.array(orderSchema),total:z.number().int().nonnegative(),page:z.number().int().positive()});
export const createdOrderSchema=z.object({order_id:z.number().int().positive(),amount,pay_amount:amount,currency:z.string().optional(),fee_rate:amount.default(0),bonus_amount:amount.optional(),expires_at:z.string(),qr_code:z.string().nullish(),can_open:z.boolean()});
export type Checkout=z.infer<typeof checkoutSchema>;
export type Plan=z.infer<typeof planSchema>;
export type PaymentOrder=z.infer<typeof orderSchema>;
export type CreatedOrder=z.infer<typeof createdOrderSchema>;
export interface CreateOrderRequest {amount:number;payment_type:string;order_type:'balance'|'subscription';plan_id?:number}
export const methodNames:Record<string,string>={alipay:'支付宝',wxpay:'微信支付',stripe:'银行卡 / Stripe',airwallex:'Airwallex',easypay:'在线支付'};
export const statusNames:Record<string,string>={PENDING:'待支付',PAID:'已支付',RECHARGING:'正在到账',COMPLETED:'已完成',EXPIRED:'已过期',CANCELLED:'已取消',FAILED:'支付失败',REFUND_REQUESTED:'退款申请中',REFUNDING:'退款中',REFUND_PENDING:'退款处理中',PARTIALLY_REFUNDED:'部分退款',REFUNDED:'已退款',REFUND_FAILED:'退款失败'};
export function visibleMethods(info:Checkout){
  const result:Checkout['methods']={};
  for(const [name,method] of Object.entries(info.methods)){
    const alias=name==='alipay_direct'?'alipay':name==='wxpay_direct'?'wxpay':name;
    if(!result[alias]||alias===name)result[alias]=method;
  }
  return result;
}
// Molly's deployed Sub2API uses for_sale for catalog listing, while the
// custom user_visible flag controls purchases. Listed closed plans stay visible.
export function planVisible(plan:Plan){return plan.for_sale;}
export function planPurchasable(plan:Plan){return planVisible(plan)&&plan.user_visible;}
export function currency(value?:string){return /^[A-Z]{3}$/.test(value?.toUpperCase()??'')?value!.toUpperCase():'CNY';}
export function money(value:number,code='CNY'){return new Intl.NumberFormat('zh-CN',{style:'currency',currency:currency(code),currencyDisplay:'narrowSymbol'}).format(value);}
export function quote(info:Checkout,input:number,method:string,plan?:Plan){
  const code=currency(visibleMethods(info)[method]?.currency);
  const digits=new Intl.NumberFormat('zh-CN',{style:'currency',currency:code}).resolvedOptions().maximumFractionDigits??2;
  const factor=10**digits, round=(n:number)=>Math.round(n*factor)/factor;
  const valid=Number.isFinite(input)&&input>0?input:0;
  const multiplier=info.balance_recharge_multiplier>0?info.balance_recharge_multiplier:1;
  let base=valid, credit=Math.round(valid*multiplier*100)/100, bonus=0;
  if(plan){base=round(plan.price*(code==='CNY'&&info.subscription_usd_to_cny_rate>0?info.subscription_usd_to_cny_rate:1));credit=0;}
  else{
    const tier=[...(info.recharge_bonus_tiers??[])].sort((a,b)=>b.min_amount-a.min_amount).find(t=>valid>=t.min_amount);
    const percent=tier?.bonus_percent??0;
    if(info.recharge_bonus_mode==='discount'){
      if(percent>0&&percent<100){const discounted=round(valid*(100-percent)/100);if(discounted>0&&discounted<valid)base=discounted;}
    }else{bonus=Math.round(credit*percent)/100;credit=Math.round((credit+bonus)*100)/100;}
  }
  const feeRate=info.recharge_fee_rate??visibleMethods(info)[method]?.fee_rate??0;
  const fee=feeRate>0?Math.ceil(base*feeRate/100*(plan?factor:100))/(plan?factor:100):0;
  return {base,credit,bonus,fee,total:round(base+fee),currency:code};
}
export function amountError(info:Checkout,value:number,method:string,plan?:Plan){
  if(plan&&!planPurchasable(plan))return '此套餐暂不可购买';
  if(!Number.isFinite(value)||value<=0)return '请输入大于 0 的金额';
  const limit=visibleMethods(info)[method];
  if(!limit||!limit.available)return '请选择可用的付款方式';
  const q=quote(info,value,method,plan), compared=plan?q.total:q.base;
  if(limit.single_min>0&&compared<limit.single_min)return `此方式最低支付 ${money(limit.single_min,q.currency)}`;
  if(limit.single_max>0&&compared>limit.single_max)return `此方式最高支付 ${money(limit.single_max,q.currency)}`;
  if(limit.daily_limit>0&&limit.daily_remaining!=null&&compared>limit.daily_remaining)return '此方式今日剩余额度不足，请更换付款方式';
  return '';
}
function preview(){return import.meta.env.DEV&&!('__TAURI_INTERNALS__' in window)&&new URLSearchParams(location.search).get('ui-preview')==='console';}
function desktop(){if(!('__TAURI_INTERNALS__' in window))throw new Error('请在 MollyCloud 客户端中管理充值。');}
const demoInfo=checkoutSchema.parse({payment_enabled:true,methods:{alipay:{display_name:'支付宝',available:true,single_min:1,single_max:10000,currency:'CNY'},wxpay:{display_name:'微信支付',available:true,single_min:1,single_max:5000,currency:'CNY'},stripe:{display_name:'银行卡',available:true,currency:'USD'}},plans:[{id:1,name:'Molly Pro',description:'适合日常编程与创作',price:20,validity_days:30,validity_unit:'day',features:['OpenAI 模型通道','每日 $10 使用额度','独立订阅额度'],for_sale:true,group_platform:'openai',group_name:'Molly Pro',rate_multiplier:1,daily_limit_usd:10},{id:2,name:'Claude Plus',description:'面向长文分析与复杂任务',price:35,validity_days:30,validity_unit:'day',features:['Claude 模型通道','每日 $20 使用额度'],for_sale:true,group_platform:'anthropic',group_name:'Claude',rate_multiplier:1.2,daily_limit_usd:20},{id:3,name:'Molly 展示套餐',description:'可查看套餐权益，当前暂未开放购买',price:50,validity_days:30,features:['预览订阅权益'],for_sale:true,user_visible:false,group_platform:'openai'},{id:4,name:'隐藏套餐',price:99,validity_days:30,features:[],for_sale:false,user_visible:true}],balance_recharge_multiplier:0.14,subscription_usd_to_cny_rate:7,recharge_fee_rate:0,help_text:'充值成功后，余额将自动更新。',recharge_bonus_tiers:[{min_amount:100,bonus_percent:10}]});
let demoOrders:PaymentOrder[]=[orderSchema.parse({id:8100,amount:14,pay_amount:100,currency:'CNY',payment_type:'alipay',out_trade_no:'DEMO-8100',status:'COMPLETED',order_type:'balance',created_at:'2026-10-01T10:00:00+08:00',expires_at:'2026-10-01T10:30:00+08:00'})];
export const paymentApi={
  async checkout():Promise<Checkout>{if(preview())return structuredClone(demoInfo);desktop();return checkoutSchema.parse(await invoke('payment_checkout'));},
  async orders(page=1){if(preview())return {items:structuredClone(demoOrders.slice((page-1)*10,page*10)),total:demoOrders.length,page};desktop();return ordersSchema.parse(await invoke('payment_orders',{page}));},
  async create(request:CreateOrderRequest):Promise<CreatedOrder>{
    if(preview()){
      const plan=demoInfo.plans.find(p=>p.id===request.plan_id),q=quote(demoInfo,request.amount,request.payment_type,plan),id=Date.now();
      if(request.order_type==='subscription'&&(!plan||!planPurchasable(plan)))throw new Error('此套餐暂不可购买');
      const expires=new Date(Date.now()+30*60000).toISOString();
      demoOrders.unshift(orderSchema.parse({id,amount:plan?.price??q.credit,pay_amount:q.total,currency:q.currency,payment_type:request.payment_type,out_trade_no:`DEMO-${id}`,status:'PENDING',order_type:request.order_type,created_at:new Date().toISOString(),expires_at:expires}));
      return {order_id:id,amount:plan?.price??q.credit,pay_amount:q.total,currency:q.currency,fee_rate:0,expires_at:expires,can_open:true};
    }
    desktop();return createdOrderSchema.parse(await invoke('payment_create_order',{request}));
  },
  async order(orderId:number,verify=false):Promise<PaymentOrder>{if(preview()){const order=demoOrders.find(o=>o.id===orderId);if(!order)throw new Error('演示订单不存在');return structuredClone(order);}desktop();return orderSchema.parse(await invoke('payment_order',{orderId,verify}));},
  async cancel(orderId:number){if(preview()){const order=demoOrders.find(o=>o.id===orderId);if(order)order.status='CANCELLED';return;}desktop();await invoke('payment_cancel_order',{orderId});},
  async open(orderId:number,dark:boolean,viewId:string){desktop();await invoke('open_payment_order',{orderId,dark,viewId});},
};
