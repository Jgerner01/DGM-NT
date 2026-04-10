using System;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using DGM_NT.Models;
using DGM_NT.Services;

namespace DGM_NT.Views;

/// <summary>
/// Dialog zeigt Import-Vorschau und fordert Bestätigung
/// </summary>
public class ImportBestaetigungDialog : Window
{
    private FileFormatService.ImportErgebnis _ergebnis;
    private TextBlock _tbFormat = null!;
    private TextBlock _tbDatei = null!;
    private TextBlock _tbZeilen = null!;
    private TextBlock _tbPunkte = null!;
    private TextBlock _tbFehler = null!;
    private TextBlock _tbBounds = null!;
    private TextBlock _tbVorschau = null!;
    private CheckBox _chkImportieren = null!;
    private Button _btnOk = null!;

    public bool Importieren { get; private set; }

    public ImportBestaetigungDialog(FileFormatService.ImportErgebnis ergebnis)
    {
        _ergebnis = ergebnis;

        Title = "Punkte-Import bestätigen";
        Width = 700;
        Height = 580;
        WindowStartupLocation = WindowStartupLocation.CenterOwner;
        ResizeMode = ResizeMode.CanResizeWithGrip;
        Background = new SolidColorBrush(Color.FromRgb(245, 245, 245));

        var mainGrid = new Grid { Margin = new Thickness(20) };
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });   // Titel
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });   // Info-Grid
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });   // Warnungen
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });   // Vorschau Label
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) }); // Vorschau
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });   // Checkbox
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });   // Buttons

        // Titel
        var titel = new TextBlock
        {
            Text = "📥 Punkte-Import",
            FontSize = 20,
            FontWeight = FontWeights.Bold,
            Margin = new Thickness(0, 0, 0, 10)
        };
        Grid.SetRow(titel, 0);
        mainGrid.Children.Add(titel);

        // Info-Bereich
        var infoBorder = new Border
        {
            Background = Brushes.White,
            BorderBrush = Brushes.LightGray,
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(4),
            Padding = new Thickness(12),
            Margin = new Thickness(0, 0, 0, 10)
        };

        var infoGrid = new Grid();
        infoGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        infoGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        infoGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        infoGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        infoGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        infoGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        infoGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        infoGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        infoGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

        int row = 0;
        AddInfoRow(infoGrid, row++, "Format:", _tbFormat = new TextBlock { FontWeight = FontWeights.Medium },
                   "Datei:", _tbDatei = new TextBlock { FontWeight = FontWeights.Medium });
        AddInfoRow(infoGrid, row++, "Zeilen:", _tbZeilen = new TextBlock(),
                   "Punkte:", _tbPunkte = new TextBlock { Foreground = Brushes.Green, FontWeight = FontWeights.Bold });
        AddInfoRow(infoGrid, row++, "Fehler:", _tbFehler = new TextBlock { Foreground = Brushes.Red },
                   "Übersprungen:", new TextBlock { Foreground = Brushes.Orange });
        
        var boundsX = new TextBlock();
        var boundsY = new TextBlock();
        var boundsH = new TextBlock();
        AddInfoRow(infoGrid, row++, "Ausdehnung Y:", boundsY,
                   "Höhe:", boundsH);

        // Bounds zusammenfassen
        _tbBounds = new TextBlock();
        Grid.SetRow(_tbBounds, 4);
        Grid.SetColumn(_tbBounds, 1);
        Grid.SetColumnSpan(_tbBounds, 3);
        infoGrid.Children.Add(_tbBounds);

        infoBorder.Child = infoGrid;
        Grid.SetRow(infoBorder, 1);
        mainGrid.Children.Add(infoBorder);

        // Warnungen
        var warnPanel = new StackPanel { Margin = new Thickness(0, 0, 0, 10), Visibility = Visibility.Collapsed };
        var warnLabel = new TextBlock 
        { 
            Text = "⚠ Warnungen:", 
            FontWeight = FontWeights.Bold, 
            Foreground = Brushes.Orange,
            Margin = new Thickness(0, 0, 0, 3)
        };
        _tbVorschau = new TextBlock 
        { 
            Foreground = Brushes.DarkOrange,
            FontSize = 11,
            TextWrapping = TextWrapping.Wrap
        };
        warnPanel.Children.Add(warnLabel);
        warnPanel.Children.Add(_tbVorschau);
        Grid.SetRow(warnPanel, 2);
        mainGrid.Children.Add(warnPanel);

        // Vorschau Label
        var previewLabel = new TextBlock
        {
            Text = "📋 Vorschau (erste 15 Punkte):",
            FontWeight = FontWeights.SemiBold,
            FontSize = 13,
            Margin = new Thickness(0, 0, 0, 5)
        };
        Grid.SetRow(previewLabel, 3);
        mainGrid.Children.Add(previewLabel);

        // Vorschau-Tabelle
        var previewBorder = new Border
        {
            Background = Brushes.White,
            BorderBrush = Brushes.LightGray,
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(4),
            Padding = new Thickness(8)
        };

        var scrollViewer = new ScrollViewer { VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        _tbVorschau = new TextBlock
        {
            FontFamily = new FontFamily("Consolas"),
            FontSize = 12,
            TextWrapping = TextWrapping.NoWrap
        };
        scrollViewer.Content = _tbVorschau;
        previewBorder.Child = scrollViewer;
        Grid.SetRow(previewBorder, 4);
        mainGrid.Children.Add(previewBorder);

        // Checkbox
        var chkPanel = new StackPanel { Orientation = Orientation.Horizontal, Margin = new Thickness(0, 10, 0, 10) };
        _chkImportieren = new CheckBox
        {
            Content = "Punkte in das aktuelle Projekt importieren",
            FontSize = 13,
            FontWeight = FontWeights.Medium,
            IsChecked = true,
            Margin = new Thickness(0, 0, 0, 0)
        };
        chkPanel.Children.Add(_chkImportieren);
        Grid.SetRow(chkPanel, 5);
        mainGrid.Children.Add(chkPanel);

        // Buttons
        var btnPanel = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right };
        
        _btnOk = new Button
        {
            Content = "✓ Importieren",
            Padding = new Thickness(25, 8, 25, 8),
            FontSize = 13,
            FontWeight = FontWeights.SemiBold,
            IsDefault = true,
            Margin = new Thickness(0, 0, 10, 0),
            Background = new SolidColorBrush(Color.FromRgb(16, 124, 16)),
            Foreground = Brushes.White
        };
        _btnOk.Click += BtnOk_Click;
        btnPanel.Children.Add(_btnOk);

        var btnAbbruch = new Button { Content = "Abbruch", Padding = new Thickness(15, 8, 15, 8), IsCancel = true };
        btnAbbruch.Click += (s, e) => DialogResult = false;
        btnPanel.Children.Add(btnAbbruch);

        Grid.SetRow(btnPanel, 6);
        mainGrid.Children.Add(btnPanel);

        Content = mainGrid;

        // Daten füllen
        FuelleDaten(warnPanel);
    }

    private void AddInfoRow(Grid grid, int row, string label1Text, TextBlock value1, string label2Text, TextBlock value2)
    {
        var lbl1 = new TextBlock { Text = label1Text, Foreground = Brushes.Gray, Margin = new Thickness(0, 2, 10, 2), Width = 100 };
        var lbl2 = new TextBlock { Text = label2Text, Foreground = Brushes.Gray, Margin = new Thickness(20, 2, 10, 2), Width = 80 };

        Grid.SetRow(lbl1, row); Grid.SetColumn(lbl1, 0);
        Grid.SetRow(value1, row); Grid.SetColumn(value1, 1);
        Grid.SetRow(lbl2, row); Grid.SetColumn(lbl2, 2);
        Grid.SetRow(value2, row); Grid.SetColumn(value2, 3);

        grid.Children.Add(lbl1);
        grid.Children.Add(value1);
        grid.Children.Add(lbl2);
        grid.Children.Add(value2);
    }

    private void FuelleDaten(StackPanel warnPanel)
    {
        _tbFormat.Text = _ergebnis.FormatName;
        _tbDatei.Text = Path.GetFileName(_ergebnis.DateiPfad);
        _tbZeilen.Text = _ergebnis.AnzahlZeilen.ToString();
        _tbPunkte.Text = _ergebnis.AnzahlPunkte.ToString();
        _tbPunkte.Foreground = _ergebnis.AnzahlPunkte > 0 ? Brushes.Green : Brushes.Red;
        _tbFehler.Text = _ergebnis.AnzahlFehler > 0 ? _ergebnis.AnzahlFehler.ToString() : "0";

        if (_ergebnis.AnzahlPunkte > 0)
        {
            _tbBounds.Text = $"X: {_ergebnis.MinX:F2} bis {_ergebnis.MaxX:F2}  |  " +
                             $"Y: {_ergebnis.MinY:F2} bis {_ergebnis.MaxY:F2}  |  " +
                             $"H: {_ergebnis.MinH:F2} bis {_ergebnis.MaxH:F2}";
        }

        // Warnungen + Ausreißer
        if (_ergebnis.Warnungen.Count > 0 || _ergebnis.AnzahlAusreisser > 0 || _ergebnis.AnzahlUebersprungen > 0)
        {
            warnPanel.Visibility = Visibility.Visible;
            var warnText = new System.Text.StringBuilder();
            
            if (_ergebnis.AnzahlUebersprungen > 0)
                warnText.AppendLine($"⊘ {_ergebnis.AnzahlUebersprungen} Punkte übersprungen (Höhe=0 oder Koordinaten 0/0)");
            
            if (_ergebnis.AnzahlAusreisser > 0)
                warnText.AppendLine($"⚠ {_ergebnis.AnzahlAusreisser} Ausreißer erkannt (trotzdem eingelesen, markiert als AUSREISSER)");

            foreach (var w in _ergebnis.Warnungen)
                warnText.AppendLine(w);
            
            _tbVorschau.Text = warnText.ToString().TrimEnd();
        }

        // Vorschau (erste 15 Punkte)
        var preview = new System.Text.StringBuilder();
        preview.AppendLine($"{"Nr",-8} {"X",16} {"Y",16} {"H",10} {"Code",-8}");
        preview.AppendLine(new string('-', 62));

        int count = Math.Min(15, _ergebnis.Punkte.Count);
        for (int i = 0; i < count; i++)
        {
            var p = _ergebnis.Punkte[i];
            preview.AppendLine($"{p.PNr,-8} {p.X,16:F3} {p.Y,16:F3} {p.H,10:F3} {p.Code,-8}");
        }

        if (_ergebnis.Punkte.Count > 15)
        {
            preview.AppendLine($"... und {_ergebnis.Punkte.Count - 15} weitere Punkte");
        }

        _tbVorschau.Text = preview.ToString();
    }

    private void BtnOk_Click(object sender, RoutedEventArgs e)
    {
        if (_chkImportieren.IsChecked == true)
        {
            Importieren = true;
            DialogResult = true;
        }
        else
        {
            Importieren = false;
            DialogResult = false;
        }
    }
}
