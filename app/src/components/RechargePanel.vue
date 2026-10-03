<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { NAlert, NButton, NInputNumber, NQrCode, NSpin } from 'naive-ui';
import { paymentApi, visibleMethods, quote, amountError, currency, money, methodNames, statusNames, planVisible, planPurchasable, type Plan, type Checkout, type PaymentOrder, type CreatedOrder } from '../payments';
import { formatDate } from '../format';
import PaymentWindowStatus from './PaymentWindowStatus.vue';
import ProviderIcon from './ProviderIcon.vue';
import PaymentIcon from './PaymentIcon.vue';
import { providerTone } from '../keyGroups';
import { useSlidingSelection } from '../useSlidingSelection';
import { enterNavPanel, leaveNavPanel, cancelNavPanel } from '../navigationMotion';
const props=defineProps<{active:boolean;obscured:boolean;preview:boolean}>();
const emit=defineEmits<{refresh:[]}>();
const tab=ref<'balance'|'subscription'|'orders'>('balance'), info=ref<Checkout|null>(null);
const tabNav=ref<HTMLElement|null>(null);
useSlidingSelection(tabNav,tab);
const loading=ref(false), submitting=ref(false), checking=ref(false), cancelling=ref(false), uncertain=ref(false), error=ref(''), notice=ref('');
const amount=ref<number|null>(100), method=ref(''), planId=ref<number|null>(null);
const orders=ref<PaymentOrder[]>([]), total=ref(0), page=ref(1), ordersLoading=ref(false);
const selected=ref<PaymentOrder|null>(null), sessions=ref<Record<number,CreatedOrder>>({}), windowOrderId=ref<number|null>(null);
const paymentWindow=ref<InstanceType<typeof PaymentWindowStatus>|null>(null);
let generation=0, disposed=false, timer:number|undefined, listGeneration=0;
const methods=computed(()=>info.value?visibleMethods(info.value):{});
const plans=computed(()=>info.value?.plans.filter(planVisible)??[]);
const plan=computed(()=>tab.value==='subscription'?plans.value.find(p=>p.id===planId.value):undefined);
const payable=computed(()=>plan.value?.price??amount.value??0);
const estimate=computed(()=>info.value?quote(info.value,payable.value,method.value,plan.value):null);
const validation=computed(()=>!info.value?'':tab.value==='subscription'&&(!plan.value||!planPurchasable(plan.value))?'请选择可购买的订阅套餐':amountError(info.value,payable.value,method.value,plan.value));
const canSubmit=computed(()=>info.value?.payment_enabled&&!loading.value&&!submitting.value&&!uncertain.value&&!validation.value&&(tab.value!=='balance'||!info.value?.balance_disabled));
const session=computed(()=>selected.value?sessions.value[selected.value.id]:undefined);
const pending=computed(()=>selected.value?.status==='PENDING');
const liveStatus=(status:string)=>['PENDING','PAID','RECHARGING'].includes(status);
function canChoosePlan(item:Plan){return Boolean(info.value?.payment_enabled)&&planPurchasable(item);}
function message(e:unknown){return e instanceof Error?e.message:String(e);}
async function load(){
  if(loading.value)return;const current=++generation;loading.value=true;error.value='';
  try{
    const result=await paymentApi.checkout();if(disposed||current!==generation)return;
    info.value=result;
    if(!methods.value[method.value])method.value=Object.keys(methods.value).find(m=>methods.value[m]?.available)??'';
    if(result.balance_disabled&&tab.value==='balance')tab.value='subscription';
    if(planId.value!==null&&!plans.value.some(p=>p.id===planId.value&&canChoosePlan(p)))planId.value=null;
  }catch(e){if(!disposed&&current===generation)error.value=message(e);}
  finally{if(current===generation)loading.value=false;}
}
async function loadOrders(){
  const current=++listGeneration;ordersLoading.value=true;
  try{const data=await paymentApi.orders(page.value);if(disposed||current!==listGeneration)return;if(data.items.some(item=>item.status==='COMPLETED'&&orders.value.some(old=>old.id===item.id&&liveStatus(old.status))))emit('refresh');orders.value=data.items;total.value=data.total;}
  catch(e){if(!disposed&&current===listGeneration)error.value=message(e);}
  finally{if(current===listGeneration)ordersLoading.value=false;}
}
async function changePage(next:number){page.value=next;await loadOrders();}
async function create(){
  if(!canSubmit.value)return;submitting.value=true;error.value='';notice.value='';
  try{
    const request={amount:payable.value,payment_type:method.value,order_type:tab.value==='subscription'?'subscription' as const:'balance' as const,...(plan.value?{plan_id:plan.value.id}:{})};
    const result=await paymentApi.create(request);if(disposed)return;
    sessions.value[result.order_id]=result;
    selected.value={id:result.order_id,amount:result.amount,pay_amount:result.pay_amount,currency:result.currency,fee_rate:result.fee_rate,payment_type:request.payment_type,out_trade_no:'',status:'PENDING',order_type:request.order_type,created_at:new Date().toISOString(),expires_at:result.expires_at,refund_amount:0};
    tab.value='orders';page.value=1;
    if(result.can_open)openPayment(result.order_id);
    await check(false);await loadOrders();
  }catch(e){if(!disposed){error.value=message(e);uncertain.value=true;tab.value='orders';await loadOrders();}}
  finally{submitting.value=false;}
}
async function check(verify=true){
  if(!selected.value||checking.value)return;checking.value=true;const id=selected.value.id;
  try{
    const result=await paymentApi.order(id,verify);if(disposed||selected.value?.id!==id)return;
    const previous=selected.value.status;selected.value=result;
    const idx=orders.value.findIndex(o=>o.id===id);if(idx>=0)orders.value[idx]=result;
    if(result.status!=='PENDING'&&windowOrderId.value===id)windowOrderId.value=null;
    if(result.status==='COMPLETED'&&previous!=='COMPLETED'){notice.value=result.order_type==='subscription'?'订阅购买成功，账户信息已刷新。':'充值已到账，余额已刷新。';emit('refresh');}
  }catch(e){if(!disposed)error.value=message(e);}
  finally{checking.value=false;}
}
async function cancel(){
  if(!selected.value||!pending.value||cancelling.value)return;cancelling.value=true;error.value='';
  try{await paymentApi.cancel(selected.value.id);if(windowOrderId.value===selected.value.id)windowOrderId.value=null;await check(false);await loadOrders();}
  catch(e){if(!disposed)error.value=message(e);}
  finally{cancelling.value=false;}
}
function view(order:PaymentOrder){if(cancelling.value||checking.value)return;selected.value=order;void check(false);}
function dismiss(){selected.value=null;}
function openPayment(orderId:number){
  if(windowOrderId.value===orderId)void paymentWindow.value?.open();
  else windowOrderId.value=orderId;
}
async function paymentWindowReturned(){await check(false);await loadOrders();}
watch(tab,value=>{if(!uncertain.value)error.value='';if(value==='orders')void loadOrders();else{selected.value=null;} });
watch(()=>props.active,active=>{if(active&&!info.value)void load();},{immediate:true});
watch(()=>[props.active,props.obscured,selected.value?.id,selected.value?.status,tab.value],()=>{
  window.clearInterval(timer);
  if(props.active&&!props.obscured){
    timer=window.setInterval(()=>{
      if(document.visibilityState!=='visible'||checking.value||ordersLoading.value||cancelling.value)return;
      if(selected.value&&liveStatus(selected.value.status))void check(false);
      else if(tab.value==='orders'&&orders.value.some(o=>liveStatus(o.status)))void loadOrders();
    },8000);
  }
},{immediate:true});
onBeforeUnmount(()=>{disposed=true;++generation;++listGeneration;window.clearInterval(timer);});
</script>
<template>
  <section class="recharge-panel" aria-label="充值与订阅">
    <p v-if="preview" class="payment-demo-note">界面预览 · 以下为演示数据，不产生真实付款</p>
    <div ref="tabNav" class="payment-tabs" role="tablist" aria-label="充值类型"><i class="selection-indicator" aria-hidden="true" /><button v-for="item in [{id:'balance',name:'余额充值'},{id:'subscription',name:'订阅套餐'},{id:'orders',name:'我的订单'}]" :key="item.id" :id="`payment-tab-${item.id}`" type="button" role="tab" :class="{active:tab===item.id}" :aria-selected="tab===item.id" :aria-controls="`payment-panel-${item.id}`" :disabled="submitting" @click="tab=item.id as typeof tab">{{item.name}}</button></div>
    <PaymentWindowStatus v-if="windowOrderId!==null" ref="paymentWindow" :key="windowOrderId" :order-id="windowOrderId" :preview="preview" @refresh="paymentWindowReturned" @close="windowOrderId=null"/>
    <n-alert v-if="error" type="error" :bordered="false" :show-icon="false" role="alert">{{error}}<n-button v-if="!info&&!loading" text @click="load">重试</n-button></n-alert>
    <n-alert v-if="notice" type="success" :bordered="false" :show-icon="false" role="status">{{notice}}</n-alert>
    <n-alert v-if="uncertain" type="warning" :bordered="false" :show-icon="false">下单结果未确认。请先核对订单记录，避免重复下单。<n-button text @click="loadOrders">刷新订单</n-button><n-button text @click="uncertain=false">已核对，允许重新下单</n-button></n-alert>
    <Transition :css="false" mode="out-in" @enter="enterNavPanel" @leave="leaveNavPanel" @enter-cancelled="cancelNavPanel" @leave-cancelled="cancelNavPanel">
    <div :key="tab" :id="`payment-panel-${tab}`" class="payment-tab-content" role="tabpanel" :aria-labelledby="`payment-tab-${tab}`">
    <div v-if="loading&&!info" class="payment-loading"><n-spin size="small"/> 正在获取充值信息…</div>
    <template v-else-if="tab!=='orders'&&info">
      <n-alert v-if="!info.payment_enabled" type="info" :bordered="false" :show-icon="false">暂未开放在线支付，已有订单可在“我的订单”中查看。</n-alert>
      <n-alert v-else-if="tab==='balance'&&info.balance_disabled" type="info" :bordered="false" :show-icon="false">暂未开放余额充值，请选择订阅套餐。</n-alert>
      <template v-if="tab==='subscription'||(info.payment_enabled&&!info.balance_disabled)">
        <p v-if="tab==='balance'&&info.recharge_bonus_notice" class="payment-help">{{info.recharge_bonus_notice}}</p>
        <div v-if="tab==='subscription'" class="payment-plans">
          <button v-for="item in plans" :key="item.id" class="payment-plan" :class="{'is-selected':planId===item.id}" type="button" :aria-pressed="planId===item.id" :disabled="submitting||!canChoosePlan(item)" :data-purchasable="canChoosePlan(item)" @click="planId=item.id">
            <span class="provider-badge" :data-provider="providerTone(item.group_platform??'')"><ProviderIcon :platform="item.group_platform"/><strong>{{item.group_name||item.name}}</strong><span v-if="item.rate_multiplier!=null">{{item.rate_multiplier}}x</span></span>
            <strong class="payment-plan-name">{{item.name}}</strong><span class="payment-plan-description">{{item.description}}</span>
            <span class="payment-plan-price">{{money(item.price,item.currency||'USD')}}<small> / {{item.validity_days}}{{item.validity_unit==='month'?'个月':item.validity_unit==='year'?'年':'天'}}</small></span>
            <span v-for="feature in item.features" :key="feature" class="payment-plan-feature">✓ {{feature}}</span>
            <span class="payment-plan-select">{{!canChoosePlan(item)?'不可购买':planId===item.id?'已选择':'选择套餐'}}</span>
          </button>
          <p v-if="!plans.length" class="payment-empty">暂无可展示的订阅套餐。</p>
        </div>
        <div v-if="info.payment_enabled" class="payment-compose">
          <div class="payment-card payment-form">
            <template v-if="tab==='balance'">
              <h3>充值金额 <small>{{currency(methods[method]?.currency)}}</small></h3>
              <div class="payment-amounts"><button v-for="value in [10,50,100,200,500,1000]" :key="value" type="button" :class="{'is-selected':amount===value}" :aria-pressed="amount===value" :disabled="submitting" @click="amount=value">{{money(value,methods[method]?.currency)}}</button></div>
              <label class="native-field"><span>自定义金额</span><n-input-number v-model:value="amount" :min="0.01" :precision="2" :show-button="false" :disabled="submitting" aria-label="充值金额" placeholder="输入充值金额"/></label>
            </template>
            <h3>付款方式</h3><div class="payment-methods"><button v-for="(item,key) in methods" :key="key" type="button" :data-method="key" :aria-pressed="method===key" :class="{'is-selected':method===key}" :disabled="!item.available||submitting" @click="method=key"><PaymentIcon v-if="key==='alipay'||key==='wxpay'" :method="key"/><span>{{item.display_name||methodNames[key]||key}}</span></button><p v-if="!Object.keys(methods).length" class="payment-empty">暂无可用的付款方式。</p></div>
          </div>
          <aside class="payment-card payment-summary">
            <h3>订单预览</h3><template v-if="estimate"><dl><div><dt>{{tab==='balance'?'充值金额':'订阅套餐'}}</dt><dd>{{plan?.name??money(amount??0,estimate.currency)}}</dd></div><div v-if="estimate.base!==(amount??0)||plan"><dt>支付基数</dt><dd>{{money(estimate.base,estimate.currency)}}</dd></div><div><dt>手续费</dt><dd>{{money(estimate.fee,estimate.currency)}}</dd></div><div v-if="tab==='balance'"><dt>预计到账 <small>USD</small></dt><dd>{{money(estimate.credit,'USD')}}</dd></div><div v-if="estimate.bonus>0"><dt>含赠送额度</dt><dd>{{money(estimate.bonus,'USD')}}</dd></div></dl><div class="payment-total"><span>预计实付</span><strong>{{money(estimate.total,estimate.currency)}}</strong></div></template>
            <p class="payment-fineprint">金额以创建订单后的服务端确认为准。</p><p v-if="validation" class="payment-validation">{{validation}}</p>
            <n-button type="primary" size="large" block :disabled="!canSubmit" :loading="submitting" @click="create">创建订单并支付</n-button>
          </aside>
        </div>
        <p v-if="info.help_text" class="payment-help">{{info.help_text}}</p>
      </template>
    </template>
    <div v-else-if="tab==='orders'" class="payment-orders">
      <section v-if="selected" id="payment-order-detail" class="payment-card payment-detail" aria-label="订单详情">
        <header><div><h3>订单 #{{selected.id}}</h3><p>{{statusNames[selected.status]||selected.status}} · {{selected.order_type==='subscription'?'订阅购买':'余额充值'}}</p></div><n-button size="small" quaternary @click="dismiss">收起</n-button></header>
        <div class="payment-detail-summary"><span>实付金额 <strong>{{money(selected.pay_amount,selected.currency)}}</strong></span><span v-if="selected.order_type==='balance'">到账额度 <strong>{{money(selected.amount,'USD')}}</strong></span><span v-if="pending">支付截止 {{formatDate(selected.expires_at)}}</span></div>
        <div v-if="pending&&session?.qr_code" class="payment-qr"><n-qr-code :value="session.qr_code" :size="176" color="#182014" background-color="#ffffff"/><p>请使用对应支付应用扫码<br/>{{methodNames[selected.payment_type]||selected.payment_type}} · {{money(selected.pay_amount,selected.currency)}}</p></div>
        <p v-if="pending&&!session" class="payment-fineprint">此订单的支付会话已结束。可查询到账状态，或取消未支付订单后重新下单。</p>
        <p v-if="pending&&session&&!session.qr_code&&!session.can_open" class="payment-fineprint">服务端未返回桌面端可用的付款入口，请先查询订单状态。</p>
        <div class="payment-detail-actions"><n-button v-if="pending&&session?.can_open" type="primary" :disabled="cancelling" @click="openPayment(selected.id)">打开支付窗口</n-button><n-button :loading="checking" :disabled="cancelling" @click="check(true)">{{pending?'我已支付，查询状态':'刷新状态'}}</n-button><n-button v-if="pending" :loading="cancelling" :disabled="checking" @click="cancel">取消订单</n-button></div>

      </section>
      <div class="payment-orders-heading"><h3>订单记录 <small>{{total}}</small></h3><n-button size="small" secondary :loading="ordersLoading" @click="loadOrders">刷新订单</n-button></div>
      <div class="payment-order-list" :aria-busy="ordersLoading">
        <button v-for="order in orders" :key="order.id" type="button" class="payment-order-row" :disabled="cancelling||checking" :aria-expanded="selected?.id===order.id" :aria-controls="selected?.id===order.id?'payment-order-detail':undefined" :aria-label="`订单 ${order.id}，${statusNames[order.status]||order.status}，${money(order.pay_amount,order.currency)}，打开详情`" @click="view(order)">
          <span><strong>{{order.order_type==='subscription'?'订阅购买':'余额充值'}}</strong><small>#{{order.id}} · {{formatDate(order.created_at)}}</small></span>
          <span><strong>{{money(order.pay_amount,order.currency)}}</strong><small>{{methodNames[order.payment_type]||order.payment_type}}</small></span>
          <span class="payment-status" :data-status="order.status">{{statusNames[order.status]||order.status}}</span>
        </button>
        <p v-if="!orders.length&&!ordersLoading" class="payment-empty">暂无订单记录。</p>
      </div>
      <div class="payment-pagination"><n-button size="small" :disabled="page<=1||ordersLoading" @click="changePage(page-1)">上一页</n-button><span>{{page}} / {{Math.max(1,Math.ceil(total/10))}}</span><n-button size="small" :disabled="page*10>=total||ordersLoading" @click="changePage(page+1)">下一页</n-button></div>
    </div>
    </div>
    </Transition>
  </section>
</template>
