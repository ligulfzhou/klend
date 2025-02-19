use anchor_lang::{
    prelude::*,
    solana_program::sysvar::{instructions::Instructions as SysInstructions, SysvarId},
    Accounts,
};
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface};

use crate::{
    check_refresh_ixs,
    lending_market::{lending_checks, lending_operations},
    state::{obligation::Obligation, LendingMarket, Reserve},
    utils::{token_transfer, FatAccountLoader},
    xmsg, LendingAction, ReserveFarmKind,
};

pub fn process(
    ctx: Context<RepayObligationLiquidity>,
    liquidity_amount: u64,
    skip_farm_check: bool,
) -> Result<()> {
    if !skip_farm_check {
        check_refresh_ixs!(
            ctx.accounts,
            ctx.accounts.repay_reserve,
            ReserveFarmKind::Debt
        );
    }
    lending_checks::repay_obligation_liquidity_checks(&ctx)?;

    let clock = Clock::get()?;

    let repay_reserve = &mut ctx.accounts.repay_reserve.load_mut()?;
    let obligation = &mut ctx.accounts.obligation.load_mut()?;
    let lending_market = &ctx.accounts.lending_market.load()?;

    let initial_reserve_token_balance = token_interface::accessor::amount(
        &ctx.accounts.reserve_destination_liquidity.to_account_info(),
    )?;
    let initial_reserve_available_liquidity = repay_reserve.liquidity.available_amount;

    let repay_amount = lending_operations::repay_obligation_liquidity(
        repay_reserve,
        obligation,
        &clock,
        liquidity_amount,
        ctx.accounts.repay_reserve.key(),
        lending_market,
        ctx.remaining_accounts.iter().map(|a| {
            FatAccountLoader::try_from(a).expect("Remaining account is not a valid deposit reserve")
        }),
    )?;

    xmsg!(
        "pnl: Repaying obligation liquidity {} liquidity_amount {}",
        repay_amount,
        liquidity_amount
    );

    token_transfer::repay_obligation_liquidity_transfer(
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.reserve_liquidity_mint.to_account_info(),
        ctx.accounts.user_source_liquidity.to_account_info(),
        ctx.accounts.reserve_destination_liquidity.to_account_info(),
        ctx.accounts.owner.to_account_info(),
        repay_amount,
        ctx.accounts.reserve_liquidity_mint.decimals,
    )?;

    lending_checks::post_transfer_vault_balance_liquidity_reserve_checks(
        token_interface::accessor::amount(
            &ctx.accounts.reserve_destination_liquidity.to_account_info(),
        )
        .unwrap(),
        repay_reserve.liquidity.available_amount,
        initial_reserve_token_balance,
        initial_reserve_available_liquidity,
        LendingAction::Additive(repay_amount),
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct RepayObligationLiquidity<'info> {
    pub owner: Signer<'info>,

    #[account(mut,
        has_one = lending_market,
        constraint = obligation.load()?.lending_market == repay_reserve.load()?.lending_market
    )]
    pub obligation: AccountLoader<'info, Obligation>,

    pub lending_market: AccountLoader<'info, LendingMarket>,

    #[account(mut,
        has_one = lending_market
    )]
    pub repay_reserve: AccountLoader<'info, Reserve>,

    #[account(mut,
        address = repay_reserve.load()?.liquidity.mint_pubkey,
        mint::token_program = token_program,
    )]
    pub reserve_liquidity_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut,
        address = repay_reserve.load()?.liquidity.supply_vault,
    )]
    pub reserve_destination_liquidity: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut,
        token::mint = repay_reserve.load()?.liquidity.mint_pubkey
    )]
    pub user_source_liquidity: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,

    #[account(address = SysInstructions::id())]
    pub instruction_sysvar_account: AccountInfo<'info>,
}
